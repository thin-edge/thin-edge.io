use async_trait::async_trait;
use tedge_api::plugin::HandleTypes;
use tedge_api::Plugin;
use tedge_api::PluginBuilder;
use tedge_api::PluginConfiguration;
use tedge_api::PluginDirectory;
use tedge_api::PluginError;
use tedge_core::configuration::TedgeConfiguration;
use tedge_core::errors::TedgeApplicationError;
use tedge_core::TedgeApplication;

pub struct VerifyConfigFailsPluginBuilder;

#[async_trait::async_trait]
impl<PD: PluginDirectory> PluginBuilder<PD> for VerifyConfigFailsPluginBuilder {
    fn kind_name() -> &'static str {
        "verify_config_fails"
    }

    async fn verify_configuration(
        &self,
        _config: &PluginConfiguration,
    ) -> Result<(), tedge_api::error::PluginError> {
        Err(tedge_api::error::PluginError::Custom(anyhow::anyhow!(
            "Verification of config failed"
        )))
    }

    async fn instantiate(
        &self,
        _config: PluginConfiguration,
        _cancellation_token: tedge_api::CancellationToken,
        _plugin_dir: &PD,
    ) -> Result<tedge_api::plugin::BuiltPlugin, PluginError> {
        unreachable!()
    }

    fn kind_message_types() -> HandleTypes
    where
        Self: Sized,
    {
        HandleTypes::empty()
    }
}

struct VerifyConfigFailsPlugin;

#[async_trait]
impl Plugin for VerifyConfigFailsPlugin {
    #[allow(unreachable_code)]
    async fn setup(&mut self) -> Result<(), PluginError> {
        unreachable!()
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        unreachable!()
    }
}

#[tokio::test]
async fn test_verify_fails_plugin() -> Result<(), Box<(dyn std::error::Error + 'static)>> {
    let _ = tracing_subscriber::fmt::try_init();

    const CONF: &'static str = r#"
        communication_buffer_size = 10

        plugin_shutdown_timeout_ms = 2000

        [plugins]

        [plugins.no_verify_plugin]
        kind = "verify_config_fails"
        [plugins.no_verify_plugin.configuration]
    "#;

    let config: TedgeConfiguration = toml::de::from_str(CONF)?;
    let (_cancel_sender, application) = TedgeApplication::builder()
        .with_plugin_builder(VerifyConfigFailsPluginBuilder {})?
        .with_config(config)?;

    match application.run().await {
        Err(TedgeApplicationError::PluginConfigVerificationFailed(e)) => {
            tracing::info!("Application errored successfully: {:?}", e);
            Ok(())
        }
        _ => {
            panic!("Application should have errored because plugin failed to verify configuration")
        }
    }
}
