use futures::StreamExt;

use tedge_api::Plugin;

use crate::TedgeApplication;
use crate::configuration::PluginInstanceConfiguration;
use crate::errors::Result;
use crate::errors::TedgeApplicationError;

/// Helper type for running a TedgeApplication
///
/// This type is only introduced for more seperation-of-concerns in the codebase
/// `Reactor::run()` is simply `TedgeApplication::run()`.
pub struct Reactor(pub TedgeApplication);

impl Reactor {
    pub async fn run(self) -> Result<()> {
        self.verify_configurations().await
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
}

