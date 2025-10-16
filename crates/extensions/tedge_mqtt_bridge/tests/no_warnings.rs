use anyhow::Context;
use rumqttc::AsyncClient;
use rumqttc::EventLoop;
use rumqttc::MqttOptions;
use rumqttc::QoS;
use rumqttd::Broker;
use rumqttd::Config;
use rumqttd::ConnectionSettings;
use rumqttd::ServerSettings;
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tedge_config::TEdgeConfig;
use tedge_mqtt_bridge::BridgeConfig;
use tedge_mqtt_bridge::MqttBridgeActorBuilder;
use tokio::net::TcpListener;
use tokio::time::sleep;

const HEALTH: &str = "te/device/main/#";

#[tokio::test]
async fn bridge_should_not_log_warnings_during_normal_operation() {
    let log_capture = TestLogCapture::new();

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(log_capture.clone())
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set subscriber");

    let local_broker_port = free_port().await;
    let cloud_broker_port = free_port().await;
    let (local, mut ev_local) = new_broker_and_client("local", local_broker_port);
    let (_cloud, _ev_cloud) = new_broker_and_client("cloud", cloud_broker_port);

    let mut rules = BridgeConfig::new();
    rules.forward_from_local("s/us", "c8y/", "").unwrap();
    rules.forward_from_remote("s/ds", "c8y/", "").unwrap();

    // We need to wait until the brokers are actually listening before starting
    // the bridge otherwise the bridge will log connection errors
    wait_until_port_listening(local_broker_port).await;
    wait_until_port_listening(cloud_broker_port).await;

    start_mqtt_bridge(local_broker_port, cloud_broker_port, rules).await;

    local.subscribe(HEALTH, QoS::AtLeastOnce).await.unwrap();

    wait_until_health_status_is("up", &mut ev_local)
        .await
        .unwrap();

    local.unsubscribe(HEALTH).await.unwrap();

    // Send a few messages to ensure the bridge is working
    local
        .publish("c8y/s/us", QoS::AtLeastOnce, false, "test message")
        .await
        .unwrap();

    // Give some time for messages to be processed and healthcheck to complete
    sleep(Duration::from_millis(500)).await;

    // Verify that log capture is working by checking for INFO logs
    // This ensures the relevant subscriber has been set up correctly
    assert!(
        log_capture.has_info_logs(),
        "Log capture is not working! This test must run in its own test file/process. \
         If this test was moved to bridge.rs or another shared test file, it will produce \
         false negatives because env_logger from other tests takes precedence if registered \
         first."
    );

    // Check that no warnings were logged
    if log_capture.has_warnings() {
        panic!(
            "Bridge logged warnings during normal operation:\n{}",
            log_capture
                .get_logs()
                .into_iter()
                .filter(|log| log.contains("WARN"))
                .collect::<String>()
        );
    }
}

fn new_broker_and_client(name: &str, port: u16) -> (AsyncClient, EventLoop) {
    let mut broker = Broker::new(get_rumqttd_config(port));
    std::thread::Builder::new()
        .name(format!("{name} broker"))
        .spawn(move || broker.start().unwrap())
        .unwrap();
    let mut client_opts = MqttOptions::new(format!("{name}-test-client"), "127.0.0.1", port);
    client_opts.set_max_packet_size(268435455, 268435455);
    AsyncClient::new(client_opts, 10)
}

async fn start_mqtt_bridge(local_port: u16, cloud_port: u16, rules: BridgeConfig) {
    let cloud_config = MqttOptions::new("a-device-id", "127.0.0.1", cloud_port);
    let service_name = "tedge-mapper-test";
    let health_topic = format!("te/device/main/service/{service_name}/status/health")
        .as_str()
        .try_into()
        .unwrap();
    MqttBridgeActorBuilder::new(
        &tedge_mqtt_config(local_port),
        service_name,
        &health_topic,
        rules,
        cloud_config,
        None,
    )
    .await;
}

async fn wait_until_port_listening(port: u16) {
    let mut attempts = 0;
    let max_attempts = 1000;
    let delay = Duration::from_millis(10);

    while attempts < max_attempts {
        match tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)).await {
            Ok(_) => return,
            Err(_) => {
                attempts += 1;
                tokio::time::sleep(delay).await;
            }
        }
    }
    panic!(
        "Failed to connect to port {} after {} attempts",
        port, max_attempts
    );
}

async fn wait_until_health_status_is(
    status: &str,
    event_loop: &mut EventLoop,
) -> anyhow::Result<()> {
    use rumqttc::Event;
    use rumqttc::Incoming;
    use std::str::from_utf8;
    use tokio::time::timeout;

    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

    loop {
        let response = timeout(DEFAULT_TIMEOUT, event_loop.poll())
            .await
            .context("timed-out waiting for health message")?;

        if let Ok(Event::Incoming(Incoming::Publish(publish))) = response {
            if publish.topic.starts_with("te/device/main/service")
                && publish.topic.ends_with("status/health")
            {
                let payload = from_utf8(&publish.payload).context("decoding health payload")?;
                let json: serde_json::Value = serde_json::from_str(payload)?;
                match (status, json["status"].as_str()) {
                    ("up", Some("up")) | ("down", Some("down")) => break Ok(()),
                    (_, Some("up" | "down")) => continue,
                    _ => continue,
                }
            }
        }
    }
}

async fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn tedge_mqtt_config(mqtt_port: u16) -> TEdgeConfig {
    TEdgeConfig::load_toml_str(&format!(
        "
    mqtt.client.port = {mqtt_port}
    mqtt.bridge.reconnect_policy.initial_interval = \"0s\"
    "
    ))
}

#[derive(Clone)]
struct TestLogCapture {
    logs: Arc<Mutex<Vec<String>>>,
}

impl TestLogCapture {
    fn new() -> Self {
        Self {
            logs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn has_warnings(&self) -> bool {
        self.logs
            .lock()
            .unwrap()
            .iter()
            .any(|log| log.contains("WARN"))
    }

    fn has_info_logs(&self) -> bool {
        self.logs
            .lock()
            .unwrap()
            .iter()
            .any(|log| log.contains("INFO"))
    }

    fn get_logs(&self) -> Vec<String> {
        self.logs.lock().unwrap().clone()
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for TestLogCapture {
    type Writer = TestWriter;

    fn make_writer(&'a self) -> Self::Writer {
        TestWriter {
            logs: self.logs.clone(),
            buf: Vec::new(),
        }
    }
}

struct TestWriter {
    logs: Arc<Mutex<Vec<String>>>,
    buf: Vec<u8>,
}

impl std::io::Write for TestWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.buf.is_empty() {
            if let Ok(s) = String::from_utf8(self.buf.clone()) {
                self.logs.lock().unwrap().push(s);
            }
            self.buf.clear()
        }
        Ok(())
    }
}

impl Drop for TestWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

fn get_rumqttd_config(port: u16) -> Config {
    let router_config = rumqttd::RouterConfig {
        max_segment_size: 10240,
        max_segment_count: 10,
        max_connections: 10,
        initialized_filters: None,
        ..Default::default()
    };

    let connections_settings = ConnectionSettings {
        connection_timeout_ms: 1000,
        max_payload_size: 268435455,
        max_inflight_count: 200,
        auth: None,
        dynamic_filters: false,
        external_auth: None,
    };

    let server_config = ServerSettings {
        name: port.to_string(),
        listen: ([127, 0, 0, 1], port).into(),
        tls: None,
        next_connection_delay_ms: 1,
        connections: connections_settings,
    };

    let mut servers = HashMap::new();
    servers.insert("1".to_string(), server_config);

    rumqttd::Config {
        id: 0,
        router: router_config,
        cluster: None,
        console: None,
        v4: Some(servers),
        ws: None,
        v5: None,
        bridge: None,
        prometheus: None,
        metrics: None,
    }
}
