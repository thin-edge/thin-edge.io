use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
};

use librumqttd::{Broker, Config, ConnectionSettings, ConsoleSettings, ServerSettings};
use once_cell::sync::Lazy;

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
}

fn spawn_broker(port: u16) {
    let config = get_rumqttd_config(port);
    let mut broker = Broker::new(config);
    let mut tx = broker.link("localclient").unwrap();

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
        let mut rx = tx.connect(200).unwrap();
        tx.subscribe("#").unwrap();

        loop {
            if let Some(message) = rx.recv().unwrap() {
                let mut bytes: Vec<u8> = vec![];
                let len = message.payload.len();
                for chunk in message.payload.into_iter() {
                    for byte in chunk.into_iter() {
                        bytes.push(byte);
                    }
                }

                let payload = match std::str::from_utf8(bytes.as_ref()) {
                    Ok(payload) => format!("{:.30}", payload),
                    Err(_) => format!("Non uft8 ({} bytes)", len),
                };
                eprintln!(
                    "MQTT-TEST MSG: topic = {}, payload = {:?}",
                    message.topic, payload
                );
            }
        }
    });
}

fn get_rumqttd_config(port: u16) -> Config {
    let router_config = rumqttlog::Config {
        id: 0,
        dir: "/tmp/rumqttd".into(),
        max_segment_size: 10240,
        max_segment_count: 10,
        max_connections: 10,
    };

    let connections_settings = ConnectionSettings {
        connection_timeout_ms: 1,
        max_client_id_len: 256,
        throttle_delay_ms: 0,
        max_payload_size: 268435455,
        max_inflight_count: 200,
        max_inflight_size: 1024,
        username: None,
        password: None,
    };

    let server_config = ServerSettings {
        listen: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port)),
        cert: None,
        next_connection_delay_ms: 1,
        connections: connections_settings,
    };

    let mut servers = HashMap::new();
    servers.insert("1".to_string(), server_config);

    let console_settings = ConsoleSettings {
        listen: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 3030)),
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
