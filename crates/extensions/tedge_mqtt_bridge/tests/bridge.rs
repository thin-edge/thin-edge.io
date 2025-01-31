use anyhow::anyhow;
use anyhow::Context;
use rumqttc::AsyncClient;
use rumqttc::Event;
use rumqttc::EventLoop;
use rumqttc::Incoming;
use rumqttc::MqttOptions;
use rumqttc::Outgoing;
use rumqttc::Publish;
use rumqttc::QoS;
use rumqttd::Broker;
use rumqttd::Config;
use rumqttd::ConnectionSettings;
use rumqttd::ServerSettings;
use std::collections::HashMap;
use std::str::from_utf8;
use std::time::Duration;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;
use tedge_mqtt_bridge::BridgeConfig;
use tedge_mqtt_bridge::MqttBridgeActorBuilder;
use tedge_test_utils::fs::TempTedgeDir;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::sleep;
use tokio::time::timeout;
use tracing::info;
use tracing::warn;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

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
    )
    .await;
}

const HEALTH: &str = "te/device/main/#";

// TODO acknowledgement with lost connection bridge, check we acknowledge the correct message
#[tokio::test]
async fn bridge_many_messages() {
    std::env::set_var("RUST_LOG", "rumqttd=debug,tedge_mqtt_bridge=info");
    let _ = env_logger::try_init();
    let local_broker_port = free_port().await;
    let cloud_broker_port = free_port().await;
    let (local, mut ev_local) = new_broker_and_client("local", local_broker_port);
    let (cloud, ev_cloud) = new_broker_and_client("cloud", cloud_broker_port);

    // We can't easily restart rumqttd, so instead, we'll connect via a proxy
    // that we can interrupt the connection of
    let cloud_proxy = Proxy::start(cloud_broker_port).await;

    let mut rules = BridgeConfig::new();
    rules.forward_from_local("s/us", "c8y/", "").unwrap();
    rules.forward_from_remote("s/ds", "c8y/", "").unwrap();

    start_mqtt_bridge(local_broker_port, cloud_proxy.port, rules).await;

    local.subscribe(HEALTH, QoS::AtLeastOnce).await.unwrap();

    wait_until_health_status_is("up", &mut ev_local)
        .await
        .unwrap();

    local.unsubscribe(HEALTH).await.unwrap();
    local.subscribe("c8y/s/ds", QoS::AtLeastOnce).await.unwrap();
    await_subscription(&mut ev_local).await;

    let poll_local = EventPoller::run_in_bg(ev_local);

    // Verify messages are forwarded from cloud to local
    for _ in 1..10000 {
        local
            .publish(
                "c8y/s/us",
                QoS::AtMostOnce,
                false,
                "a,fake,smartrest,message",
            )
            .await
            .unwrap();
    }

    let _ev_cloud = EventPoller::run_in_bg(ev_cloud);
    let mut local = poll_local.stop_polling().await;
    cloud
        .publish("s/ds", QoS::AtLeastOnce, false, "test")
        .await
        .unwrap();

    next_received_message(&mut local).await.unwrap();
}

#[tokio::test]
async fn bridge_forwards_large_messages() {
    std::env::set_var("RUST_LOG", "tedge_mqtt_bridge=debug,rumqttc=trace,info");
    let _ = env_logger::try_init();
    let local_broker_port = free_port().await;
    let cloud_broker_port = free_port().await;
    let (local, mut ev_local) = new_broker_and_client("local", local_broker_port);
    let (cloud, mut ev_cloud) = new_broker_and_client("cloud", cloud_broker_port);

    let mut rules = BridgeConfig::new();
    rules.forward_from_local("s/us", "c8y/", "").unwrap();
    rules.forward_from_remote("s/ds", "c8y/", "").unwrap();

    start_mqtt_bridge(local_broker_port, cloud_broker_port, rules).await;

    local.subscribe(HEALTH, QoS::AtLeastOnce).await.unwrap();

    wait_until_health_status_is("up", &mut ev_local)
        .await
        .unwrap();

    local.unsubscribe(HEALTH).await.unwrap();
    cloud.subscribe("s/us", QoS::AtLeastOnce).await.unwrap();
    await_subscription(&mut ev_cloud).await;

    let _poll_local = EventPoller::run_in_bg(ev_local);

    let payload = std::iter::repeat(b'a')
        .take(25 * 1024 * 1024)
        .collect::<Vec<u8>>();

    local
        .publish("c8y/s/us", QoS::AtLeastOnce, false, payload.clone())
        .await
        .unwrap();

    let msg = next_received_message(&mut ev_cloud).await.unwrap();
    assert_eq!(msg.payload, payload);
}

#[tokio::test]
async fn bridge_disconnect_while_sending() {
    std::env::set_var("RUST_LOG", "tedge_mqtt_bridge=info");
    let _ = env_logger::try_init();
    let local_broker_port = free_port().await;
    let cloud_broker_port = free_port().await;
    let (local, mut ev_local) = new_broker_and_client("local", local_broker_port);
    let (cloud, mut ev_cloud) = new_broker_and_client("cloud", cloud_broker_port);

    // We can't easily restart rumqttd, so instead, we'll connect via a proxy
    // that we can interrupt the connection of
    let cloud_proxy = Proxy::start(cloud_broker_port).await;

    let mut rules = BridgeConfig::new();
    rules.forward_from_local("s/us", "c8y/", "").unwrap();
    rules.forward_from_remote("s/ds", "c8y/", "").unwrap();

    start_mqtt_bridge(local_broker_port, cloud_proxy.port, rules).await;

    local.subscribe(HEALTH, QoS::AtLeastOnce).await.unwrap();

    wait_until_health_status_is("up", &mut ev_local)
        .await
        .unwrap();

    local.unsubscribe(HEALTH).await.unwrap();
    local.subscribe("c8y/s/ds", QoS::AtLeastOnce).await.unwrap();
    await_subscription(&mut ev_local).await;
    cloud.subscribe("s/us", QoS::AtLeastOnce).await.unwrap();
    await_subscription(&mut ev_cloud).await;

    let poll_local = EventPoller::run_in_bg(ev_local);

    // Verify messages are forwarded from cloud to local
    for i in 1..10000 {
        local
            .publish(
                "c8y/s/us",
                QoS::AtMostOnce,
                false,
                format!("a,fake,smartrest,message{i}"),
            )
            .await
            .unwrap();
    }
    cloud_proxy.interrupt_connections();
    let _ev_cloud = EventPoller::run_in_bg(ev_cloud);
    for _ in 1..10000 {
        local
            .publish(
                "c8y/s/us",
                QoS::AtMostOnce,
                false,
                "a,fake,smartrest,message",
            )
            .await
            .unwrap();
    }

    let mut local = poll_local.stop_polling().await;
    cloud
        .publish("s/ds", QoS::AtLeastOnce, true, "test")
        .await
        .unwrap();

    next_received_message(&mut local).await.unwrap();
}

#[tokio::test]
async fn bridge_reconnects_successfully_after_cloud_connection_interrupted() {
    std::env::set_var("RUST_LOG", "tedge_mqtt_bridge=info");
    let _ = env_logger::try_init();
    let local_broker_port = free_port().await;
    let cloud_broker_port = free_port().await;
    let (local, mut ev_local) = new_broker_and_client("local", local_broker_port);
    let (cloud, mut ev_cloud) = new_broker_and_client("cloud", cloud_broker_port);

    // We can't easily restart rumqttd, so instead, we'll connect via a proxy
    // that we can interrupt the connection of
    let cloud_proxy = Proxy::start(cloud_broker_port).await;

    let mut rules = BridgeConfig::new();
    rules.forward_from_local("s/us", "c8y/", "").unwrap();
    rules.forward_from_remote("s/ds", "c8y/", "").unwrap();
    start_mqtt_bridge(local_broker_port, cloud_proxy.port, rules).await;

    local.subscribe(HEALTH, QoS::AtLeastOnce).await.unwrap();
    cloud.subscribe("s/us", QoS::AtLeastOnce).await.unwrap();

    wait_until_health_status_is("up", &mut ev_local)
        .await
        .unwrap();
    cloud_proxy.interrupt_connections();
    wait_until_health_status_is("down", &mut ev_local)
        .await
        .unwrap();
    wait_until_health_status_is("up", &mut ev_local)
        .await
        .unwrap();

    local.unsubscribe(HEALTH).await.unwrap();
    local.subscribe("c8y/s/ds", QoS::AtLeastOnce).await.unwrap();

    await_subscription(&mut ev_local).await;
    await_subscription(&mut ev_cloud).await;

    let poll_cloud = EventPoller::run_in_bg(ev_cloud);

    // Verify messages are forwarded from cloud to local
    cloud
        .publish("s/ds", QoS::AtLeastOnce, false, "a,fake,smartrest,message")
        .await
        .unwrap();

    let msg = next_received_message(&mut ev_local).await.unwrap();
    assert_eq!(msg.topic, "c8y/s/ds");
    assert_eq!(from_utf8(&msg.payload).unwrap(), "a,fake,smartrest,message");

    let mut ev_cloud = poll_cloud.stop_polling().await;
    EventPoller::run_in_bg(ev_local);

    // Verify messages are forwarded from local to cloud
    local
        .publish("c8y/s/us", QoS::AtLeastOnce, false, "a,different,message")
        .await
        .unwrap();

    let msg = next_received_message(&mut ev_cloud).await.unwrap();
    assert_eq!(msg.topic, "s/us");
    assert_eq!(from_utf8(&msg.payload).unwrap(), "a,different,message");
}

#[tokio::test]
async fn bridge_reconnects_successfully_after_local_connection_interrupted() {
    std::env::set_var("RUST_LOG", "tedge_mqtt_bridge=info,bridge=info");
    let _ = env_logger::try_init();
    let local_broker_port = free_port().await;
    let cloud_broker_port = free_port().await;
    let (local, mut ev_local) = new_broker_and_client("local", local_broker_port);
    let (cloud, mut ev_cloud) = new_broker_and_client("cloud", cloud_broker_port);

    // We can't easily restart rumqttd, so instead, we'll connect via a proxy
    // that we can interrupt the connection of
    let local_proxy = Proxy::start(local_broker_port).await;

    let mut rules = BridgeConfig::new();
    rules.forward_from_local("s/us", "c8y/", "").unwrap();
    rules.forward_from_remote("s/ds", "c8y/", "").unwrap();
    start_mqtt_bridge(local_proxy.port, cloud_broker_port, rules).await;

    local.subscribe(HEALTH, QoS::AtLeastOnce).await.unwrap();
    cloud.subscribe("s/us", QoS::AtLeastOnce).await.unwrap();

    wait_until_health_status_is("up", &mut ev_local)
        .await
        .unwrap();

    // TODO (flaky): Investigate why adding this sleep makes the test more reliable.
    // Current theory: If a sub-ack is not received, then the subscription
    // is not remembered by the client and not resubscribed after
    // a connection outage
    sleep(Duration::from_millis(100)).await;

    local_proxy.interrupt_connections();
    wait_until_health_status_is("up", &mut ev_local)
        .await
        .unwrap();

    local.unsubscribe(HEALTH).await.unwrap();
    local.subscribe("c8y/s/ds", QoS::AtLeastOnce).await.unwrap();

    await_subscription(&mut ev_local).await;
    await_subscription(&mut ev_cloud).await;

    let poll_cloud = EventPoller::run_in_bg(ev_cloud);

    // Verify messages are forwarded from cloud to local
    cloud
        .publish("s/ds", QoS::AtLeastOnce, false, "a,fake,smartrest,message")
        .await
        .unwrap();

    let msg = next_received_message(&mut ev_local).await.unwrap();
    assert_eq!(msg.topic, "c8y/s/ds");
    assert_eq!(from_utf8(&msg.payload).unwrap(), "a,fake,smartrest,message");

    let mut ev_cloud = poll_cloud.stop_polling().await;
    EventPoller::run_in_bg(ev_local);

    // Verify messages are forwarded from local to cloud
    local
        .publish("c8y/s/us", QoS::AtLeastOnce, false, "a,different,message")
        .await
        .unwrap();

    let msg = next_received_message(&mut ev_cloud).await.unwrap();
    assert_eq!(msg.topic, "s/us");
    assert_eq!(from_utf8(&msg.payload).unwrap(), "a,different,message");
}

#[tokio::test]
async fn bidirectional_forwarding_avoids_infinite_loop() {
    let local_port = free_port().await;
    let cloud_port = free_port().await;
    let (local_client, mut local_ev_loop) = new_broker_and_client("local", local_port);
    let (cloud_client, mut cloud_ev_loop) = new_broker_and_client("cloud", cloud_port);
    let mut rules = BridgeConfig::new();
    rules
        // The cloud prefix in practice is `$aws/things/<device-id>`, but rumqttd doesn't
        // support $aws as it's not the aws broker
        .forward_bidirectionally("shadow/#", "aws/", "aws/things/my-device/")
        .unwrap();

    start_mqtt_bridge(local_port, cloud_port, rules).await;

    local_client
        .subscribe(HEALTH, QoS::AtLeastOnce)
        .await
        .unwrap();

    wait_until_health_status_is("up", &mut local_ev_loop)
        .await
        .unwrap();

    local_client.unsubscribe(HEALTH).await.unwrap();

    local_client
        .subscribe("aws/#", QoS::AtLeastOnce)
        .await
        .unwrap();
    await_subscription(&mut local_ev_loop).await;

    cloud_client
        .subscribe("aws/things/my-device/shadow/#", QoS::AtLeastOnce)
        .await
        .unwrap();
    await_subscription(&mut cloud_ev_loop).await;

    cloud_client
        .publish(
            "aws/things/my-device/shadow/request",
            QoS::AtMostOnce,
            false,
            "test message",
        )
        .await
        .unwrap();

    let cloud = tokio::spawn(async move {
        loop {
            match cloud_ev_loop.poll().await.unwrap() {
                // Ignore the initial request message
                Event::Incoming(Incoming::Publish(Publish { pkid: 1, .. })) => (),
                Event::Incoming(Incoming::Publish(publish)) => {
                    assert_eq!(
                        publish.topic, "aws/things/my-device/shadow/response",
                        "We should receive the response, the request should not be forwarded back"
                    );
                    assert_eq!(from_utf8(&publish.payload).unwrap(), "test response");
                    break;
                }
                Event::Outgoing(Outgoing::Publish(_)) => {}
                _ => (),
            }
        }
    });

    loop {
        match timeout(DEFAULT_TIMEOUT, local_ev_loop.poll())
            .await
            .context("Timed out waiting for local client to receive forwarded message")
            .unwrap()
            .unwrap()
        {
            Event::Incoming(Incoming::Publish(publish)) => {
                assert_eq!(publish.topic, "aws/shadow/request");
                assert_eq!(from_utf8(&publish.payload).unwrap(), "test message");
                local_client
                    .publish(
                        "aws/shadow/response",
                        QoS::AtLeastOnce,
                        false,
                        "test response",
                    )
                    .await
                    .unwrap();
            }
            Event::Outgoing(Outgoing::Publish(_)) => break,
            _ => (),
        }
    }

    timeout(DEFAULT_TIMEOUT, cloud).await.unwrap().unwrap();
}

async fn wait_until_health_status_is(
    status: &str,
    event_loop: &mut EventLoop,
) -> anyhow::Result<()> {
    loop {
        let health = next_received_message(event_loop).await.with_context(|| {
            format!("expecting health message waiting for status to be {status:?}")
        })?;
        if !(health.topic.starts_with("te/device/main/service")
            && health.topic.ends_with("status/health"))
        {
            warn!(
                "Unexpected message on topic {} when looking for health status messages",
                health.topic
            );
        }
        let payload = from_utf8(&health.payload).context("decoding health payload")?;
        let json: serde_json::Value = serde_json::from_str(payload)?;
        match (status, json["status"].as_str()) {
            ("up", Some("up")) | ("down", Some("down")) => break Ok(()),
            (_, Some("up" | "down")) => continue,
            (_, Some(status)) => {
                break Err(anyhow!(
                    "Unknown health status {status:?} in tedge-json: {payload}"
                ))
            }
            (_, None) => break Err(anyhow!("Health status missing from payload: {payload}")),
        }
    }
}

/// A TCP proxy that allows the connection to be dropped upon request
///
/// This is used to simulate a dropped connection between the client and MQTT broker
struct Proxy {
    port: u16,
    stop_tx: tokio::sync::watch::Sender<()>,
}

impl Proxy {
    async fn start(target_port: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(());
        tokio::spawn(async move {
            let target = format!("127.0.0.1:{target_port}");
            loop {
                let mut stop = stop_rx.clone();
                stop.mark_unchanged();
                if let Ok((mut socket, _)) = listener.accept().await {
                    let mut conn = loop {
                        let Ok(conn) = tokio::net::TcpStream::connect(&target).await else {
                            continue;
                        };
                        break conn;
                    };
                    tokio::spawn(async move {
                        let (mut read_socket, mut write_socket) = socket.split();
                        let (mut read_conn, mut write_conn) = conn.split();

                        tokio::select! {
                            _ = tokio::io::copy(&mut read_socket, &mut write_conn) => (),
                            _ = tokio::io::copy(&mut read_conn, &mut write_socket) => (),
                            _ = stop.changed() => info!("shutting down proxy"),
                        };

                        write_socket.shutdown().await.unwrap();
                        let _ = write_conn.shutdown().await;
                    });
                }
            }
        });

        Self { port, stop_tx }
    }

    /// Sends a signal to drop all active connections to the proxy
    fn interrupt_connections(&self) {
        self.stop_tx.send(()).unwrap()
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

/// A wrapper around an event loop that allows temporary polling
struct EventPoller {
    tx: oneshot::Sender<()>,
    rx: oneshot::Receiver<EventLoop>,
}

impl EventPoller {
    /// Executes the event loop in a spawned task
    pub fn run_in_bg(mut event_loop: EventLoop) -> Self {
        let (tx_stop_polling, mut rx_stop_polling) = oneshot::channel();
        let (tx_ev_loop, rx_ev_loop) = oneshot::channel();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Not quite sure why I need to use biased here,
                    // but it doesn't poll the event loop if we don't
                    biased;
                    _ = event_loop.poll() => (),
                    res = &mut rx_stop_polling => {
                        if res.is_err() {
                            return
                        } else {
                            break
                        }
                    },
                }
            }
            tx_ev_loop
                .send(event_loop)
                .ok()
                .expect("Channel should still be open");
        });

        Self {
            tx: tx_stop_polling,
            rx: rx_ev_loop,
        }
    }

    /// Stops the spawned task from polling the loop, and returns the loop for re-use
    pub async fn stop_polling(self) -> EventLoop {
        self.tx.send(()).unwrap();
        self.rx.await.unwrap()
    }
}

async fn await_subscription(event_loop: &mut EventLoop) {
    loop {
        if let Ok(Event::Incoming(Incoming::SubAck(_))) =
            timeout(DEFAULT_TIMEOUT, event_loop.poll())
                .await
                .context("timed-out waiting for subscription")
                .unwrap()
        {
            break;
        }
    }
}

async fn next_received_message(event_loop: &mut EventLoop) -> anyhow::Result<Publish> {
    loop {
        let response = timeout(DEFAULT_TIMEOUT, event_loop.poll())
            .await
            .context("timed-out waiting for received message")?;

        match response {
            // Incoming messages
            Ok(Event::Incoming(Incoming::Publish(publish))) => break Ok(publish),
            Ok(Event::Incoming(Incoming::ConnAck(v))) => {
                info!("Incoming::ConnAck: ({:?}, {})", v.code, v.session_present)
            }
            Ok(Event::Incoming(Incoming::Connect(v))) => info!(
                "Incoming::Connect: client_id={}, clean_session={}",
                v.client_id, v.clean_session
            ),
            Ok(Event::Incoming(Incoming::Disconnect)) => info!("Incoming::Disconnect"),
            Ok(Event::Incoming(Incoming::PingReq)) => info!("Incoming::PingReq"),
            Ok(Event::Incoming(Incoming::PingResp)) => info!("Incoming::PingResp"),
            Ok(Event::Incoming(Incoming::PubAck(v))) => info!("Incoming::PubAck: pkid={}", v.pkid),
            Ok(Event::Incoming(Incoming::PubComp(v))) => {
                info!("Incoming::PubComp: pkid={}", v.pkid)
            }
            Ok(Event::Incoming(Incoming::PubRec(v))) => info!("Incoming::PubRec: pkid={}", v.pkid),
            Ok(Event::Incoming(Incoming::PubRel(v))) => info!("Incoming::PubRel: pkid={}", v.pkid),
            Ok(Event::Incoming(Incoming::SubAck(v))) => info!("Incoming::SubAck: pkid={}", v.pkid),
            Ok(Event::Incoming(Incoming::Subscribe(v))) => {
                info!("Incoming::Subscribe: pkid={}", v.pkid)
            }
            Ok(Event::Incoming(Incoming::UnsubAck(v))) => {
                info!("Incoming::UnsubAck: pkid={}", v.pkid)
            }
            Ok(Event::Incoming(Incoming::Unsubscribe(v))) => {
                info!("Incoming::Unsubscribe: pkid={}", v.pkid)
            }

            // Outgoing messages
            Ok(Event::Outgoing(Outgoing::PingReq)) => info!("Outgoing::PingReq"),
            Ok(Event::Outgoing(Outgoing::PingResp)) => info!("Outgoing::PingResp"),
            Ok(Event::Outgoing(Outgoing::Publish(v))) => info!("Outgoing::Publish: pkid={v}"),
            Ok(Event::Outgoing(Outgoing::Subscribe(v))) => info!("Outgoing::Subscribe: pkid={v}"),
            Ok(Event::Outgoing(Outgoing::Unsubscribe(v))) => {
                info!("outgoing Unsubscribe: pkid={v}")
            }
            Ok(Event::Outgoing(Outgoing::PubAck(v))) => {
                info!("Outgoing::PubAck: pkid={v}")
            }
            Ok(Event::Outgoing(Outgoing::PubRec(v))) => info!("Outgoing::PubRec: pkid={v}"),
            Ok(Event::Outgoing(Outgoing::PubRel(v))) => info!("Outgoing::PubRel: pkid={v}"),
            Ok(Event::Outgoing(Outgoing::PubComp(v))) => info!("Outgoing::PubComp: pkid={v}"),
            Ok(Event::Outgoing(Outgoing::Disconnect)) => info!("Outgoing::Disconnect"),
            Ok(Event::Outgoing(Outgoing::AwaitAck(v))) => info!("Outgoing::AwaitAck: pkid={v}"),
            Err(err) => {
                info!("Connection error (ignoring). {err}");
            }
        }
    }
}

fn tedge_mqtt_config(mqtt_port: u16) -> TEdgeConfig {
    let ttd = TempTedgeDir::new();
    let config_loc = TEdgeConfigLocation::from_custom_root(ttd.path());
    config_loc
        .update_toml(&|dto, _reader| {
            dto.mqtt.client.port = Some(mqtt_port.try_into().unwrap());
            dto.mqtt.bridge.reconnect_policy.initial_interval = Some("0s".parse().unwrap());
            Ok(())
        })
        .unwrap();
    TEdgeConfig::try_new(config_loc).unwrap()
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
