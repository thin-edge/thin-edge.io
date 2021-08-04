use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
};

use librumqttd::{async_locallink, Config, ConnectionSettings, ConsoleSettings, ServerSettings};
use port_scanner::scan_port;

pub async fn start_broker_local(port: u16) -> anyhow::Result<()> {
    if !scan_port(58585) {
        //let config: Config = confy::load_path("tests/rumqttd_config/rumqttd_58585.conf")?;
        let config: Config = get_rumqttd_config(port);
        let (mut router, _console, servers, _builder) = async_locallink::construct_broker(config);
        let router =
            tokio::task::spawn_blocking(move || -> anyhow::Result<()> { Ok(router.start()?) });
        servers.await;
        let _ = router.await;
    }
    Ok(())
}

fn get_rumqttd_config(port: u16) -> librumqttd::Config {
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
