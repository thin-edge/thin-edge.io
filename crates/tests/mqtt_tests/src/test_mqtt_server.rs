use std::collections::HashMap;
use std::net::TcpListener;
use std::time::Duration;

use futures::channel::mpsc::UnboundedReceiver;
use librumqttd::Broker;
use librumqttd::Config;
use librumqttd::ConnectionSettings;
use librumqttd::ConsoleSettings;
use librumqttd::ServerSettings;
use once_cell::sync::Lazy;
use rumqttc::QoS;

static SERVER: Lazy<MqttProcessHandler> = Lazy::new(MqttProcessHandler::new);

pub fn test_mqtt_broker() -> &'static MqttProcessHandler {
    Lazy::force(&SERVER)
}

pub struct MqttProcessHandler {
    pub port: u16,
}

impl MqttProcessHandler {
    pub fn new() -> MqttProcessHandler {
        let port = spawn_broker();
        MqttProcessHandler { port }
    }

    pub async fn publish(&self, topic: &str, payload: &str) -> Result<(), anyhow::Error> {
        crate::test_mqtt_client::publish(self.port, topic, payload, QoS::AtLeastOnce, false).await
    }

    pub async fn publish_with_opts(
        &self,
        topic: &str,
        payload: &str,
        qos: QoS,
        retain: bool,
    ) -> Result<(), anyhow::Error> {
        crate::test_mqtt_client::publish(self.port, topic, payload, qos, retain).await
    }

    pub async fn messages_published_on(&self, topic: &str) -> UnboundedReceiver<String> {
        crate::test_mqtt_client::messages_published_on(self.port, topic).await
    }

    pub async fn wait_for_response_on_publish(
        &self,
        pub_topic: &str,
        pub_message: &str,
        sub_topic: &str,
        timeout: Duration,
    ) -> Option<String> {
        crate::test_mqtt_client::wait_for_response_on_publish(
            self.port,
            pub_topic,
            pub_message,
            sub_topic,
            timeout,
        )
        .await
    }

    pub fn map_messages_background<F>(&self, func: F)
    where
        F: 'static + Send + Sync + Fn((String, String)) -> Vec<(String, String)>,
    {
        tokio::spawn(crate::test_mqtt_client::map_messages_loop(self.port, func));
    }
}

impl Default for MqttProcessHandler {
    fn default() -> Self {
        Self::new()
    }
}

fn spawn_broker() -> u16 {
    // We can get a free port from the kernel by binding on port 0. We can then
    // immediately drop the listener, and use the port for the mqtt broker.
    // Unfortunately we can run into a race condition whereas when tests are run
    // in parallel, when we get a certain free port, after dropping the listener
    // the port is freed, and `TcpListener::bind` in another test might pick it
    // up before we start the mqtt broker. For this reason, we keep retrying
    // the operation if the port is already in use.
    //
    // This would have been much easier if rumqttd would just let us query the
    // port the server got after we've passed in 0 as the port but currently
    // it's not possible, so we have to rely on this workaround.
    let (mut tx, port) = loop {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();

        let config = get_rumqttd_config(port);
        let mut broker = Broker::new(config);
        let tx = broker.link("localclient").unwrap();

        // `broker.start()` blocks, so to catch a TCP port bind error we have to
        // start it in a thread and wait a bit.
        let broker_thread = std::thread::spawn(move || {
            eprintln!("MQTT-TEST INFO: start test MQTT broker (port = {})", port);
            broker.start()
        });
        std::thread::sleep(std::time::Duration::from_millis(50));

        if !broker_thread.is_finished() {
            break (tx, port);
        }

        match broker_thread.join().unwrap() {
            Ok(()) => unreachable!("`broker.start()` does not terminate"),
            Err(err) => {
                eprintln!(
                    "MQTT-TEST ERROR: fail to start the test MQTT broker: {:?}",
                    err
                );
            }
        }
    };

    std::thread::spawn(move || {
        let mut rx = tx.connect(200).unwrap();
        tx.subscribe("#").unwrap();

        loop {
            if let Some(message) = rx.recv().unwrap() {
                for chunk in message.payload.into_iter() {
                    let mut bytes: Vec<u8> = vec![];
                    for byte in chunk.into_iter() {
                        bytes.push(byte);
                    }
                    let payload = match std::str::from_utf8(bytes.as_ref()) {
                        Ok(payload) => format!("{:.110}", payload),
                        Err(_) => format!("Non uft8 ({} bytes)", bytes.len()),
                    };
                    eprintln!(
                        "MQTT-TEST MSG: topic = {}, payload = {:?}",
                        message.topic, payload
                    );
                }
            }
        }
    });

    port
}

fn get_rumqttd_config(port: u16) -> Config {
    let router_config = librumqttd::rumqttlog::Config {
        id: 0,
        dir: "/tmp/rumqttd".into(),
        max_segment_size: 10240,
        max_segment_count: 10,
        max_connections: 10,
    };

    let connections_settings = ConnectionSettings {
        connection_timeout_ms: 10,
        max_client_id_len: 256,
        throttle_delay_ms: 0,
        max_payload_size: 268435455,
        max_inflight_count: 200,
        max_inflight_size: 1024,
        login_credentials: None,
    };

    let server_config = ServerSettings {
        listen: ([0, 0, 0, 0], port).into(),
        cert: None,
        next_connection_delay_ms: 1,
        connections: connections_settings,
    };

    let mut servers = HashMap::new();
    servers.insert("1".to_string(), server_config);

    let console_settings = ConsoleSettings {
        listen: ([0, 0, 0, 0], 0).into(),
    };

    librumqttd::Config {
        id: 0,
        router: router_config,
        servers,
        cluster: None,
        replicator: None,
        console: console_settings,
    }
}
