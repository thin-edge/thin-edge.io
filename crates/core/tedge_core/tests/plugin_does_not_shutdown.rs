use async_trait::async_trait;
use futures::future::FutureExt;
use tedge_api::plugin::PluginExt;
use tedge_api::Plugin;
use tedge_api::PluginBuilder;
use tedge_api::PluginConfiguration;
use tedge_api::PluginDirectory;
use tedge_api::PluginError;
use tedge_core::configuration::TedgeConfiguration;
use tedge_core::TedgeApplication;

pub struct NoShutdownPluginBuilder;

#[async_trait::async_trait]
impl<PD: PluginDirectory> PluginBuilder<PD> for NoShutdownPluginBuilder {
    fn kind_name() -> &'static str {
        "noshutdown"
    }

    async fn verify_configuration(
        &self,
        _config: &PluginConfiguration,
    ) -> Result<(), tedge_api::error::PluginError> {
        Ok(())
    }

    async fn instantiate(
        &self,
        _config: PluginConfiguration,
        _cancellation_token: tedge_api::CancellationToken,
        _plugin_dir: &PD,
    ) -> Result<tedge_api::plugin::BuiltPlugin, PluginError> {
        Ok(NoShutdownPlugin {}.into_untyped::<()>())
    }

    fn kind_message_types() -> tedge_api::plugin::HandleTypes
    where
        Self: Sized,
    {
        tedge_api::plugin::HandleTypes::declare_handlers_for::<(), NoShutdownPlugin>()
    }
}

struct NoShutdownPlugin;

#[async_trait]
impl Plugin for NoShutdownPlugin {
    async fn setup(&mut self) -> Result<(), PluginError> {
        tracing::info!("Setup called");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        tracing::info!("Shutdown called");
        loop {
            // hot waiting
        }
    }
}

#[test]
fn test_no_shutdown_plugin() -> Result<(), Box<(dyn std::error::Error + 'static)>> {
    let _ = tracing_subscriber::fmt::try_init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let res = rt.block_on(async {
        const CONF: &'static str = r#"
            communication_buffer_size = 10

            plugin_shutdown_timeout_ms = 2000

            [plugins]
            [plugins.noshut]
            kind = "noshutdown"
            [plugins.noshut.configuration]
        "#;

        let config: TedgeConfiguration = toml::de::from_str(CONF)?;
        let (cancel_sender, application) = TedgeApplication::builder()
            .with_plugin_builder(NoShutdownPluginBuilder {})?
            .with_config(config)?;

        let mut run_fut = tokio::spawn(application.run());

        // send a cancel request to the app after 1 sec
        let mut cancel_fut = Box::pin({
            tokio::time::sleep(std::time::Duration::from_secs(1)).then(|_| async {
                tracing::info!("Cancelling app now");
                cancel_sender.cancel_app()
            })
        });

        // Abort the test after 5 secs, because it seems not to stop the application
        let mut test_abort = Box::pin({
            tokio::time::sleep(std::time::Duration::from_secs(5)).then(|_| async {
                tracing::info!("Aborting test");
            })
        });

        let mut cancelled = false;
        loop {
            tracing::info!("Looping");
            tokio::select! {
                _test_abort = &mut test_abort => {
                    tracing::error!("Test aborted");
                    run_fut.abort();
                    anyhow::bail!("Timeout reached, shutdown did not happen")
                },

                _ = &mut run_fut => {
                    tracing::info!("application.run() returned");
                    assert!(cancelled, "Application returned but cancel did not happen yet");
                    // cancel happened... everything fine.
                    break;
                },

                _ = &mut cancel_fut, if !cancelled => {
                    tracing::info!("Cancellation happened...");
                    cancelled = true;
                }
            }
        }

        Ok(())
    });

    rt.shutdown_background();
    if let Err(e) = res {
        panic!("{e:?}");
    }
    Ok(())
}
