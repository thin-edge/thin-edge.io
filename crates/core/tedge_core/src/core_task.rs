use std::collections::HashMap;

use tedge_api::address::EndpointKind;

use crate::task::Task;

type Receiver = tokio::sync::mpsc::Receiver<tedge_api::messages::Message>;
type Sender = tokio::sync::mpsc::Sender<tedge_api::messages::Message>;

pub struct CoreTask {
    recv: Receiver,
    plugin_senders: HashMap<String, Sender>,
}

impl CoreTask {
    pub fn new(recv: Receiver, plugin_senders: HashMap<String, Sender>) -> Self {
        Self {
            recv,
            plugin_senders,
        }
    }
}

#[async_trait::async_trait]
impl Task for CoreTask {
    async fn run(mut self) -> crate::errors::Result<()> {
        while let Some(message) = self.recv.recv().await {
            match message.origin().endpoint() {
                EndpointKind::Plugin { id } => {
                    log::trace!("Received message in core, routing to {}", id);
                    if let Some(sender) = self.plugin_senders.get(id) {
                        match sender.send(message).await {
                            Ok(()) => log::trace!("Sent successfully"),
                            Err(e) => log::trace!("Error sending message: {:?}", e),
                        }
                    }
                }

                EndpointKind::Core => {
                    log::trace!("Received message in core");

                    // TODO: Implement
                }
            }

        }

        Ok(())
    }
}
