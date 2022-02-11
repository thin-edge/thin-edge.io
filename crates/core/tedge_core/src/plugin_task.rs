use tedge_api::Plugin;
use tedge_api::messages::Message;

use crate::errors::Result;
use crate::errors::TedgeApplicationError;
use crate::task::Task;

type Sender = tokio::sync::mpsc::Sender<tedge_api::messages::Message>;
type Receiver = tokio::sync::mpsc::Receiver<tedge_api::messages::Message>;

pub struct PluginTask {
    plugin_name: String,
    plugin: Box<dyn Plugin>,
    plugin_message_receiver: Receiver,
    tasks_receiver: Receiver,
    plugin_task_senders: HashMap<String, Sender>,
    core_msg_sender: Sender,
}

impl PluginTask {
    pub fn new(
        plugin_name: String,
        plugin: Box<dyn Plugin>,
        plugin_message_receiver: Receiver,
        tasks_receiver: Receiver,
        plugin_task_senders: HashMap<String, Sender>,
        core_msg_sender: Sender,
    ) -> Self {
        Self {
            plugin_name,
            plugin,
            plugin_message_receiver,
            tasks_receiver,
            plugin_task_senders,
            core_msg_sender,
        }
    }

    async fn receive_only_from_other_tasks(mut self) -> Result<()> {
        while let Some(msg) = self.tasks_receiver.recv().await {
            self.handle_message_to_plugin(msg).await?;
        }

        self.plugin
            .shutdown()
            .await
            .map_err(TedgeApplicationError::from)
    }

    async fn handle_message_from_plugin(&mut self, msg: Message) -> Result<()> {
        log::debug!("Received message from plugin {}", self.plugin_name);
        self.core_msg_sender.send(msg).await.map_err(TedgeApplicationError::from)
    }

    async fn handle_message_to_plugin(&mut self, msg: Message) -> Result<()> {
        log::debug!("Sending message to plugin {}", self.plugin_name);
        self.plugin.handle_message(msg).await.map_err(TedgeApplicationError::from)
    }
}

#[async_trait::async_trait]
impl Task for PluginTask {
    async fn run(mut self) -> Result<()> {
        loop {
            tokio::select! {
                message_from_plugin = self.plugin_message_receiver.recv() => if let Some(msg) = message_from_plugin {
                    log::debug!("Received message from the plugin that should be passed to another PluginTask");
                    self.handle_message_from_plugin(msg).await?;
                } else {
                    // If the plugin_message_receiver is closed, the plugin cannot send messages to
                    // thin-edge.
                    //
                    // This means we continue to receive only messages from other tasks and send it
                    // to the plugin, until all communication with this PluginTask is finished and
                    // then return from PluginTask::run()
                    //
                    // This is implemented in a helper function that is called here
                    log::debug!("Communication has been closed by the plugin. Continuing to only send messages to the plugin");
                    return self.receive_only_from_other_tasks().await
                },

                message_to_plugin = self.tasks_receiver.recv() => if let Some(msg) = message_to_plugin {
                    log::debug!("Received message that should be passed to the plugin");
                    self.handle_message_to_plugin(msg).await?;
                } else {
                    // If the communication _to_ this PluginTask is closed, there _cannot_ be any
                    // more communication _to_ the plugin.
                    // This means we shut down.
                    log::debug!("Communication has been closed by the other PluginTask instances");
                    log::debug!("Shutting down");
                    break
                },
            }
        }

        self.plugin
            .shutdown()
            .await
            .map_err(TedgeApplicationError::from)
    }

}
