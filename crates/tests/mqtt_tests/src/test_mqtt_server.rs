use maplit::hashmap;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::net::SocketAddrV4;
use std::time::Duration;

use futures::channel::mpsc::UnboundedReceiver;
use once_cell::sync::Lazy;
use rumqttd::Broker;
use rumqttd::Config;
use rumqttd::ConnectionSettings;
use rumqttd::ConsoleSettings;
use rumqttd::Notification;
use rumqttd::RouterConfig;
use rumqttd::ServerSettings;

use rumqttc::QoS;

const MQTT_TEST_PORT: u16 = 55555;

static SERVER: Lazy<MqttProcessHandler> = Lazy::new(|| MqttProcessHandler::new(MQTT_TEST_PORT));

pub fn test_mqtt_broker() -> &'static MqttProcessHandler {
    Lazy::force(&SERVER)
}

pub struct MqttProcessHandler {
    pub port: u16,
}

impl MqttProcessHandler {
    pub fn new(port: u16) -> MqttProcessHandler {
        spawn_broker(port);
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

fn spawn_broker(port: u16) {
    let config = get_rumqttd_config(port);
    let mut broker = Broker::new(config);
    let (mut tx, mut rx) = broker.link("localclient").unwrap();

    std::thread::spawn(move || {
        eprintln!("MQTT-TEST INFO: start test MQTT broker (port = {})", port);
        if let Err(err) = broker.start() {
            eprintln!(
                "MQTT-TEST ERROR: fail to start the test MQTT broker: {:?}",
                err
            );
        }
    });

    std::thread::spawn(move || {
        tx.subscribe("#").unwrap();
        while let Some(notification) = rx.recv().unwrap() {
            if let Notification::Forward(forward) = notification {
                let payload = match std::str::from_utf8(&forward.publish.payload) {
                    Ok(payload) => format!("{:.110}", payload),
                    Err(_) => format!("Non uft8 ({} bytes)", forward.publish.payload.len()),
                };
                eprintln!(
                    "MQTT-TEST MSG: topic = {:?}, payload = {:?}",
                    forward.publish.topic, payload
                );
            }
        }
    });
}

fn get_rumqttd_config(port: u16) -> Config {
    let console_settings = {
        let mut conset: ConsoleSettings = Default::default();
        conset.listen =
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 3030)).to_string();
        conset
    };

    let rf = RouterConfig {
        instant_ack: false,
        max_segment_size: 104857600,
        max_segment_count: 100,
        max_read_len: 10240,
        max_connections: 5000,
        initialized_filters: None,
    };

    let connections_settings = ConnectionSettings {
        connection_timeout_ms: 60000,
        throttle_delay_ms: 0,
        max_payload_size: 268435455,
        max_inflight_count: 200,
        max_inflight_size: 1024,
        dynamic_filters: false,
    };

    let server_config = ServerSettings {
        listen: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port)),
        next_connection_delay_ms: 1,
        connections: connections_settings,
        name: "mqtt_test_server".to_string(),
        tls: None,
    };

    rumqttd::Config {
        id: 4,
        router: rf,
        v4: hashmap! {"testconfig".to_string() => server_config},
        v5: hashmap! {},
        ws: hashmap! {},
        cluster: None,
        console: console_settings,
        bridge: None,
        prometheus: None,
    }
}
