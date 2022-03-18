use crate::*;
use std::sync::Arc;
use async_trait::async_trait;

#[derive(Default)]
pub struct Runtime {
    plugins: Vec<Arc<dyn Plugin>>,
}

impl Runtime {
    // Start all the registered plugin instances.
    pub async fn start(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    pub fn register(&mut self, plugin: Arc<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    pub fn instantiate<P:Plugin>(&mut self, config: impl PluginConfig<Plugin=P>) -> Result<P, RuntimeError> {
        let plugin = config.instantiate()?;
        //fixme self.register(&plugin);
        Ok(plugin)
    }
}

pub trait PluginConfig {
    type Plugin: Plugin;

    // Create a plugin from the config
    fn instantiate(self) -> Result<Self::Plugin, RuntimeError>;
}

#[async_trait]
pub trait Plugin {
    async fn start(&mut self) -> Result<(), RuntimeError>;
}
