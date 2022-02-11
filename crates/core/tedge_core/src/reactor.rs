use futures::StreamExt;

use tedge_api::Plugin;

use crate::TedgeApplication;
use crate::configuration::PluginInstanceConfiguration;
use crate::configuration::PluginKind;
use crate::errors::Result;
use crate::errors::TedgeApplicationError;
use crate::plugin_task::PluginTask;
use crate::task::Task;

/// Helper type for running a TedgeApplication
///
/// This type is only introduced for more seperation-of-concerns in the codebase
/// `Reactor::run()` is simply `TedgeApplication::run()`.
pub struct Reactor(pub TedgeApplication);

type Receiver = tokio::sync::mpsc::Receiver<tedge_api::messages::Message>;
type Sender = tokio::sync::mpsc::Sender<tedge_api::messages::Message>;

/// Helper type for preparing a PluginTask
struct PluginTaskPrep {
    name: String,
    plugin: Box<dyn Plugin>,
    plugin_recv: Receiver,
    task_sender: Sender,
    task_recv: Receiver,
    core_msg_sender: Sender,
}

impl Reactor {
    pub async fn run(self) -> Result<()> {
        self.verify_configurations().await?;

        let buf_size = self.0.config().communication_buffer_size().get();
        let (core_msg_sender, core_msg_recv) = tokio::sync::mpsc::channel(buf_size);

        let instantiated_plugins = self.0
            .config()
            .plugins()
            .iter()
            .map(|(pname, pconfig)| self.instantiate_plugin(pname, pconfig, core_msg_sender.clone()))
            .collect::<futures::stream::FuturesUnordered<_>>()
            .collect::<Vec<Result<_>>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        let running_core = {
            let plugin_senders = instantiated_plugins.iter()
                .map(|prep| (prep.name.clone(), prep.task_sender.clone()))
                .collect();
            crate::core_task::CoreTask::new(core_msg_recv, plugin_senders).run()
        };

        let running_plugins = instantiated_plugins
            .into_iter()
            .map(|prep| {
                PluginTask::new(prep.name, prep.plugin, prep.plugin_recv, prep.task_recv, prep.core_msg_sender)
            })
            .map(Task::run)
            .map(Box::pin)
            .collect::<futures::stream::FuturesUnordered<_>>() // main loop
            .collect::<Vec<Result<()>>>();

        let (plugin_res, core_res) = tokio::join!(running_plugins, running_core);

        plugin_res
            .into_iter() // result type conversion
            .collect::<Result<Vec<()>>>()
            .and_then(|_| core_res)
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

    async fn instantiate_plugin(&self, plugin_name: &str, plugin_config: &PluginInstanceConfiguration, core_msg_sender: Sender) -> Result<PluginTaskPrep> {
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

        let buf_size = self.0.config().communication_buffer_size().get();
        let (plugin_message_sender, plugin_message_receiver) = tokio::sync::mpsc::channel(buf_size);
        let (task_sender, task_receiver) = tokio::sync::mpsc::channel(buf_size);

        let comms = tedge_api::plugins::Comms::new(plugin_message_sender);


        builder.instantiate(config.clone(), comms)
            .await
            .map_err(TedgeApplicationError::from)
            .map(|plugin| PluginTaskPrep {
                name: plugin_name.to_string(),
                plugin,
                plugin_recv: plugin_message_receiver,
                task_sender,
                task_recv: task_receiver,
                core_msg_sender,
            })
    }
}

