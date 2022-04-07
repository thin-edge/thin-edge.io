//! The tedge_core crate
//!

use std::collections::HashMap;

use tedge_api::plugin::HandleTypes;
use tedge_api::PluginBuilder;
use tokio_util::sync::CancellationToken;
use tracing::debug;

mod communication;
pub mod configuration;
mod core_task;
pub mod errors;
mod plugin_task;
mod reactor;
mod task;
mod utils;

pub use crate::communication::PluginDirectory;
use crate::configuration::PluginInstanceConfiguration;
pub use crate::configuration::TedgeConfiguration;
use crate::errors::Result;
use crate::errors::TedgeApplicationError;

/// A TedgeApplication
pub struct TedgeApplication {
    config: TedgeConfiguration,
    cancellation_token: CancellationToken,
    plugin_builders: HashMap<String, (HandleTypes, Box<dyn PluginBuilder<PluginDirectory>>)>,
}

impl std::fmt::Debug for TedgeApplication {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("TedgeApplication").finish()
    }
}

impl TedgeApplication {
    pub fn builder() -> TedgeApplicationBuilder {
        TedgeApplicationBuilder {
            cancellation_token: CancellationToken::new(),
            plugin_builders: HashMap::new(),
        }
    }

    pub(crate) fn config(&self) -> &TedgeConfiguration {
        &self.config
    }

    pub(crate) fn plugin_builders(
        &self,
    ) -> &HashMap<String, (HandleTypes, Box<dyn PluginBuilder<PluginDirectory>>)> {
        &self.plugin_builders
    }

    pub(crate) fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation_token
    }

    /// Run the TedgeApplication that has been setup for running
    ///
    /// This function runs as long as there is no shutdown signalled to the application.
    ///
    /// # Note
    ///
    /// This function makes sure that the configuration is verified before the plugins are started.
    /// So there is no need to call [TedgeApplication::verify_configuration] before this.
    pub async fn run(self) -> Result<()> {
        crate::reactor::Reactor(self).run().await
    }

    /// Check whether all configured plugin kinds exist (are available in registered plugins)
    #[tracing::instrument(skip(self))]
    pub async fn verify_configurations(&self) -> Vec<(String, Result<()>)> {
        use futures::stream::StreamExt;

        debug!("Verifying configurations");
        self.config()
            .plugins()
            .iter()
            .map(
                |(plugin_name, plugin_cfg): (&String, &PluginInstanceConfiguration)| async {
                    if let Some((_, builder)) =
                        self.plugin_builders().get(plugin_cfg.kind().as_ref())
                    {
                        debug!("Verifying {}", plugin_cfg.kind().as_ref());
                        let res = builder
                            .verify_configuration(plugin_cfg.configuration())
                            .await
                            .map_err(TedgeApplicationError::from);

                        (plugin_name.to_string(), res)
                    } else {
                        (
                            plugin_name.to_string(),
                            Err(TedgeApplicationError::UnknownPluginKind(
                                plugin_cfg.kind().as_ref().to_string(),
                            )),
                        )
                    }
                },
            )
            .collect::<futures::stream::FuturesUnordered<_>>()
            .collect::<Vec<(String, Result<()>)>>()
            .await
    }
}

pub struct TedgeApplicationBuilder {
    cancellation_token: CancellationToken,
    plugin_builders: HashMap<String, (HandleTypes, Box<dyn PluginBuilder<PluginDirectory>>)>,
}

impl TedgeApplicationBuilder {
    pub fn with_plugin_builder<PB: PluginBuilder<PluginDirectory>>(
        mut self,
        builder: PB,
    ) -> Result<Self> {
        let handle_types = PB::kind_message_types();
        let kind_name = PB::kind_name();

        if self.plugin_builders.contains_key(kind_name) {
            return Err(TedgeApplicationError::PluginKindExists(
                kind_name.to_string(),
            ));
        }

        self.plugin_builders
            .insert(kind_name.to_string(), (handle_types, Box::new(builder)));
        Ok(self)
    }

    pub fn with_config(
        self,
        config: TedgeConfiguration,
    ) -> Result<(TedgeApplicationCancelSender, TedgeApplication)> {
        let cancellation = TedgeApplicationCancelSender(self.cancellation_token.clone());
        let app = TedgeApplication {
            config,
            cancellation_token: self.cancellation_token,
            plugin_builders: self.plugin_builders,
        };

        Ok((cancellation, app))
    }
}

#[derive(Clone, Debug)]
pub struct TedgeApplicationCancelSender(CancellationToken);

impl TedgeApplicationCancelSender {
    pub fn cancel_app(&self) {
        self.0.cancel()
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::*;

    mod dummy {
        use async_trait::async_trait;
        use tedge_api::plugin::{BuiltPlugin, HandleTypes, PluginExt};
        use tedge_api::{Plugin, PluginBuilder, PluginConfiguration, PluginError};

        use crate::communication::PluginDirectory;

        pub struct DummyPluginBuilder;

        #[async_trait::async_trait]
        impl PluginBuilder<PluginDirectory> for DummyPluginBuilder {
            fn kind_name() -> &'static str {
                "dummy_plugin"
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
                _plugin_dir: &PluginDirectory,
            ) -> Result<BuiltPlugin, PluginError> {
                Ok(DummyPlugin.into_untyped::<()>())
            }

            fn kind_message_types() -> HandleTypes
            where
                Self: Sized,
            {
                HandleTypes::empty()
            }
        }

        pub struct DummyPlugin;

        #[async_trait]
        impl Plugin for DummyPlugin {
            async fn setup(&mut self) -> Result<(), PluginError> {
                Ok(())
            }

            async fn shutdown(&mut self) -> Result<(), PluginError> {
                Ok(())
            }
        }
    }

    const CONFIGURATION: &str = r#"
        communication_buffer_size = 1
        plugin_shutdown_timeout_ms = 1000
        [plugins]
        [plugins.testplug]
        kind = "dummy_plugin"
        [plugins.testplug.configuration]
    "#;

    #[tokio::test]
    async fn test_creating_tedge_application() -> Result<()> {
        let config = toml::de::from_str(CONFIGURATION)?;

        let (_, _) = TedgeApplication::builder()
            .with_plugin_builder(dummy::DummyPluginBuilder {})?
            .with_config(config)?;

        Ok(())
    }
}
