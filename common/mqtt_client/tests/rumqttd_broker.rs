use librumqttd::{async_locallink, Config};

pub async fn start_broker_local(cfile: &str) -> anyhow::Result<()> {
    let config: Config = confy::load_path(cfile)?;
    let (mut router, _console, servers, _builder) = async_locallink::construct_broker(config);
    let router = tokio::task::spawn_blocking(move || -> anyhow::Result<()> { Ok(router.start()?) });
    servers.await;
    let _ = router.await;
    Ok(())
}
