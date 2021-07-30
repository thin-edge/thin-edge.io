use librumqttd::{async_locallink, Config};
use port_scanner::scan_port;

pub async fn start_broker_local() -> anyhow::Result<()> {
    if !scan_port(58585) {
        let config: Config = confy::load_path("tests/rumqttd_config/rumqttd_58585.conf")?;
        let (mut router, _console, servers, _builder) = async_locallink::construct_broker(config);
        let router =
            tokio::task::spawn_blocking(move || -> anyhow::Result<()> { Ok(router.start()?) });
        servers.await;
        let _ = router.await;
    }
    Ok(())
}
