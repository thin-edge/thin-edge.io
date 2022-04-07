use std::any::TypeId;
use std::collections::HashSet;

use futures::StreamExt;

use futures::future::FutureExt;
use tedge_api::plugin::BuiltPlugin;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use tracing::error;
use tracing::trace;

use crate::communication::PluginDirectory;
use crate::communication::PluginInfo;
use crate::configuration::PluginInstanceConfiguration;
use crate::configuration::PluginKind;
use crate::errors::Result;
use crate::errors::TedgeApplicationError;
use crate::plugin_task::PluginTask;
use crate::task::Task;
use crate::TedgeApplication;

/// Helper type for running a TedgeApplication
///
/// This type is only introduced for more seperation-of-concerns in the codebase
/// `Reactor::run()` is simply `TedgeApplication::run()`.
pub struct Reactor(pub TedgeApplication);

impl std::fmt::Debug for Reactor {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Helper type for preparing a PluginTask
struct PluginTaskPrep {
    name: String,
    plugin: BuiltPlugin,
    plugin_msg_receiver: tedge_api::address::MessageReceiver,
    cancellation_token: CancellationToken,
}

impl Reactor {
    pub async fn run(self) -> Result<()> {
        let channel_size = self.0.config().communication_buffer_size().get();

        let directory_iter = self.0.config().plugins().iter().map(|(pname, pconfig)| {
            let handle_types = self
                .0
                .plugin_builders()
                .get(pconfig.kind().as_ref())
                .map(|(handle_types, _)| {
                    handle_types
                        .get_types()
                        .into_iter()
                        .cloned()
                        .collect::<HashSet<(&'static str, TypeId)>>()
                })
                .ok_or_else(|| {
                    TedgeApplicationError::UnknownPluginKind(pconfig.kind().as_ref().to_string())
                })?;

            Ok((
                pname.to_string(),
                PluginInfo::new(handle_types, channel_size),
            ))
        });
        let (core_sender, core_receiver) = tokio::sync::mpsc::channel(channel_size);
        let mut directory = PluginDirectory::collect_from(directory_iter, core_sender)?;

        let instantiated_plugins = self
            .0
            .config()
            .plugins()
            .iter()
            .map(|(pname, pconfig)| {
                let receiver = match directory
                    .get_mut(pname)
                    .and_then(|pinfo| pinfo.receiver.take())
                {
                    Some(receiver) => receiver,
                    None => unreachable!(
                        "Tried to take receiver twice. This is a FATAL bug, please report it"
                    ),
                };

                (pname, pconfig, receiver)
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(pname, pconfig, receiver)| {
                self.instantiate_plugin(
                    pname,
                    pconfig,
                    &directory,
                    receiver,
                    self.0.cancellation_token().child_token(),
                )
            })
            .collect::<futures::stream::FuturesUnordered<_>>()
            .collect::<Vec<Result<_>>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        debug!("Plugins instantiated");

        let running_core = {
            // we clone the cancellation_token here, because the core must be able to use the
            // "root" token to stop all plugins
            let core_cancel_token = self.0.cancellation_token().clone();
            crate::core_task::CoreTask::new(core_cancel_token, core_receiver).run()
        };
        debug!("Core task instantiated");

        let running_plugins = instantiated_plugins
            .into_iter()
            .map(|prep| {
                PluginTask::new(
                    prep.name,
                    prep.plugin,
                    prep.plugin_msg_receiver,
                    prep.cancellation_token,
                    self.0.config().plugin_shutdown_timeout(),
                )
            })
            .map(Task::run)
            .map(Box::pin)
            .collect::<futures::stream::FuturesUnordered<_>>() // main loop
            .collect::<Vec<Result<()>>>()
            .inspect(|res| debug!("All Plugin Tasks finished running: {:?}", res));
        debug!("Plugin tasks instantiated");

        debug!("Entering main loop");
        let (plugin_res, core_res) = tokio::join!(running_plugins, running_core);

        plugin_res
            .into_iter() // result type conversion
            .collect::<Result<Vec<()>>>()
            .and_then(|_| core_res)
    }

    fn get_config_for_plugin<'a>(
        &'a self,
        plugin_name: &str,
    ) -> Option<&'a tedge_api::PluginConfiguration> {
        trace!("Searching config for plugin: {}", plugin_name);
        self.0
            .config()
            .plugins()
            .get(plugin_name)
            .map(|cfg| cfg.configuration())
    }

    fn find_plugin_builder<'a>(
        &'a self,
        plugin_kind: &PluginKind,
    ) -> Option<&'a Box<dyn tedge_api::PluginBuilder<PluginDirectory>>> {
        trace!("Searching builder for plugin: {}", plugin_kind.as_ref());
        self.0
            .plugin_builders()
            .get(plugin_kind.as_ref())
            .map(|(_, pb)| pb)
    }

    async fn instantiate_plugin(
        &self,
        plugin_name: &str,
        plugin_config: &PluginInstanceConfiguration,
        directory: &PluginDirectory,
        plugin_msg_receiver: tedge_api::address::MessageReceiver,
        cancellation_token: CancellationToken,
    ) -> Result<PluginTaskPrep> {
        let builder = self
            .find_plugin_builder(plugin_config.kind())
            .ok_or_else(|| {
                let kind_name = plugin_config.kind().as_ref().to_string();
                TedgeApplicationError::UnknownPluginKind(kind_name)
            })?;

        let config = self.get_config_for_plugin(plugin_name).ok_or_else(|| {
            let pname = plugin_name.to_string();
            TedgeApplicationError::PluginConfigMissing(pname)
        })?;

        if let Err(e) = builder.verify_configuration(&config).await {
            error!(
                "Verification of configuration failed for plugin '{}'",
                plugin_name
            );
            return Err(TedgeApplicationError::PluginConfigVerificationFailed(e));
        }

        let cancel_token = self.0.cancellation_token.child_token();

        trace!(
            "Instantiating plugin: {} of kind {}",
            plugin_name,
            plugin_config.kind().as_ref()
        );
        builder
            .instantiate(config.clone(), cancel_token, directory)
            .await
            .map_err(TedgeApplicationError::from)
            .map(|plugin| PluginTaskPrep {
                name: plugin_name.to_string(),
                plugin,
                plugin_msg_receiver,
                cancellation_token,
            })
    }
}
