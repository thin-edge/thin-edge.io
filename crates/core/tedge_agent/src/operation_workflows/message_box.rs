use async_trait::async_trait;
use log::error;
use std::collections::HashMap;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::Sender;
use tedge_api::commands::CmdMetaSyncSignal;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::OperationName;

/// A sender of [GenericCommandState]
/// that dispatches each command to the appropriate actor according the command's type.
#[derive(Default)]
pub(crate) struct CommandDispatcher {
    senders: HashMap<OperationName, DynSender<GenericCommandState>>,
}

#[async_trait]
impl Sender<GenericCommandState> for CommandDispatcher {
    async fn send(&mut self, message: GenericCommandState) -> Result<(), ChannelError> {
        let Some(operation) = message.operation() else {
            error!("Not an operation topic: {}", message.topic.as_ref());
            return Ok(());
        };
        let Some(sender) = self.senders.get_mut(&operation) else {
            error!("Unknown operation: {operation}");
            return Ok(());
        };
        sender.send(message).await
    }
}

impl CommandDispatcher {
    /// Register where to send commands of a given type
    pub fn register_operation_handler(
        &mut self,
        operation: OperationName,
        sender: DynSender<GenericCommandState>,
    ) {
        self.senders.insert(operation, sender);
    }

    /// List the operations for which a builtin handler has been registered
    pub fn capabilities(&self) -> Vec<OperationName> {
        self.senders.keys().cloned().collect()
    }
}

#[derive(Default)]
pub(crate) struct SyncSignalDispatcher {
    senders: HashMap<OperationType, Vec<DynSender<CmdMetaSyncSignal>>>,
}

impl SyncSignalDispatcher {
    /// Register where to send sync signals for the given command type
    pub(crate) fn register_sync_signal_sender(
        &mut self,
        operation: OperationType,
        sender: DynSender<CmdMetaSyncSignal>,
    ) {
        self.senders.entry(operation).or_default().push(sender);
    }

    pub(crate) async fn send(&mut self, operation: OperationType) -> Result<(), ChannelError> {
        let Some(senders) = self.senders.get_mut(&operation) else {
            return Ok(());
        };
        for sender in senders {
            sender.send(()).await?;
        }
        Ok(())
    }
}
