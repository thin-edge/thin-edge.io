//! The tedge_core crate
//!

use std::collections::HashMap;

use tedge_api::PluginBuilder;

pub mod configuration;
pub mod errors;
mod task;
mod core_task;
mod plugin_task;
mod reactor;
mod utils;

use crate::configuration::TedgeConfiguration;
use crate::errors::Result;
use crate::errors::TedgeApplicationError;

/// A TedgeApplication
pub struct TedgeApplication {
    config: TedgeConfiguration,
    plugin_builders: HashMap<String, Box<dyn PluginBuilder>>,
}

impl TedgeApplication {
    pub fn builder() -> TedgeApplicationBuilder {
        TedgeApplicationBuilder {
            plugin_builders: HashMap::new(),
        }
    }

    pub(crate) fn config(&self) -> &TedgeConfiguration {
        &self.config
    }

    pub(crate) fn plugin_builders(&self) -> &HashMap<String, Box<dyn PluginBuilder>> {
        &self.plugin_builders
    }

    /// Run the TedgeApplication that has been setup for running
    ///
    /// This function runs as long as there is no shutdown signalled to the application.
    pub async fn run(self) -> Result<()> {
        crate::reactor::Reactor(self).run().await
    }
}

pub struct TedgeApplicationBuilder {
    plugin_builders: HashMap<String, Box<dyn PluginBuilder>>,
}

impl TedgeApplicationBuilder {
    pub fn with_plugin_builder(mut self, builder: Box<dyn PluginBuilder>) -> Result<Self> {
        if self.plugin_builders.contains_key(builder.kind_name()) {
            return Err(TedgeApplicationError::PluginKindExists(builder.kind_name().to_string()))
        }

        self.plugin_builders.insert(builder.kind_name().to_string(), builder);
        Ok(self)
    }

    pub fn with_config(self, config: TedgeConfiguration) -> Result<TedgeApplication> {
        Ok(TedgeApplication {
            config,
            plugin_builders: self.plugin_builders,
        })
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::*;

    mod dummy {
        use async_trait::async_trait;
        use tedge_api::{
            Plugin, PluginBuilder, PluginConfiguration, PluginError, Message,
        };

        pub struct DummyPluginBuilder;

        #[async_trait::async_trait]
        impl PluginBuilder for DummyPluginBuilder {
            fn kind_name(&self) -> &'static str {
                "dummy_plugin"
            }

            async fn verify_configuration(
                &self,
                _config: &PluginConfiguration,
            ) -> Result<(), tedge_api::errors::PluginError> {
                Ok(())
            }

            async fn instantiate(
                &self,
                _config: PluginConfiguration,
                _tedge_comms: tedge_api::plugins::Comms,
            ) -> Result<Box<dyn Plugin>, PluginError> {
                Ok(Box::new(DummyPlugin))
            }
        }

        pub struct DummyPlugin;

        #[async_trait]
        impl Plugin for DummyPlugin {
            async fn setup(&mut self) -> Result<(), PluginError> {
                Ok(())
            }

            async fn handle_message(&self, _message: Message) -> Result<(), PluginError> {
                Ok(())
            }

            async fn shutdown(&mut self) -> Result<(), PluginError> {
                Ok(())
            }
        }
    }

    const CONFIGURATION: &str = r#"
    "#;


    #[tokio::test]
    async fn test_creating_tedge_application() -> Result<()> {
        let config = toml::de::from_str(CONFIGURATION)?;

        let _: TedgeApplication = TedgeApplication::builder()
            .with_plugin_builder(Box::new(dummy::DummyPluginBuilder {}))?
            .with_config(config)?;

        Ok(())
    }
}

