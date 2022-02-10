use futures::StreamExt;

use tedge_api::Plugin;

use crate::TedgeApplication;
use crate::configuration::PluginInstanceConfiguration;
use crate::configuration::PluginKind;
use crate::errors::Result;
use crate::errors::TedgeApplicationError;

/// Helper type for running a TedgeApplication
///
/// This type is only introduced for more seperation-of-concerns in the codebase
/// `Reactor::run()` is simply `TedgeApplication::run()`.
pub struct Reactor(pub TedgeApplication);

type Receiver = tokio::sync::mpsc::Receiver<tedge_api::messages::CoreMessage>;

impl Reactor {
    pub async fn run(self) -> Result<()> {
        self.verify_configurations().await?;
        let _plugins = self.instantiate_plugins().await?;
        Ok(())
    }

    /// Check whether all configured plugin kinds exist (are available in registered plugins)
    async fn verify_configurations(&self) -> Result<()> {
        self.0.config()
            .plugins()
            .values()
            .map(|plugin_cfg: &PluginInstanceConfiguration| async {
                if let Some(builder) = self.0.plugin_builders().get(plugin_cfg.kind().as_ref()) {
                    builder.verify_configuration(plugin_cfg.configuration())
                        .await
                        .map_err(TedgeApplicationError::from)
                } else {
                    unimplemented!()
                }
            })
            .collect::<futures::stream::FuturesUnordered<_>>()
            .collect::<Vec<Result<()>>>()
            .await
            .into_iter()
            .collect::<Result<()>>()
    }

    fn get_config_for_plugin<'a>(&'a self, plugin_name: &str) -> Option<&'a tedge_api::PluginConfiguration> {
        self.0.config()
            .plugins()
            .get(plugin_name)
            .map(|cfg| cfg.configuration())
    }

    fn find_plugin_builder<'a>(&'a self, plugin_kind: &PluginKind) -> Option<&'a dyn tedge_api::PluginBuilder> {
        self.0.plugin_builders()
            .get(plugin_kind.as_ref())
            .map(AsRef::as_ref)
    }

    async fn instantiate_plugins(&self) -> Result<HashMap<String, (Box<dyn Plugin>, Receiver)>> {
        self.0.config()
            .plugins()
            .iter()
            .map(|(plugin_name, plugin_config)| async {
                let builder = self.find_plugin_builder(plugin_config.kind())
                    .ok_or_else(|| {
                        let kind_name = plugin_config.kind().as_ref().to_string();
                        TedgeApplicationError::UnknownPluginKind(kind_name)
                    })?;

                let config = self.get_config_for_plugin(plugin_name)
                    .ok_or_else(|| {
                        let pname = plugin_name.to_string();
                        TedgeApplicationError::PluginConfigMissing(pname)
                    })?;

                let (sender, receiver) = tokio::sync::mpsc::channel(10); // TODO: Channel size should be configurable

                let comms = tedge_api::plugins::Comms::new(sender);

                builder.instantiate(config.clone(), comms)
                    .await
                    .map_err(TedgeApplicationError::from)
                    .map(|plugin| (plugin_name.clone(), (plugin, receiver)))
            })
            .collect::<futures::stream::FuturesUnordered<_>>()
            .collect::<Vec<Result<_>>>()
            .await // TODO: takes time until all are instantiatet, even if one fails
            .into_iter() // type conversion
            .collect::<Result<HashMap<_, (_, _)>>>()
    }
}

