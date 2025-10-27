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
    op_handler_senders: HashMap<OperationType, DynSender<CmdMetaSyncSignal>>,
    op_listener_senders: HashMap<OperationType, Vec<DynSender<CmdMetaSyncSignal>>>,
}

impl SyncSignalDispatcher {
    /// Register sender for the operation handler to receive sync signals for that operation
    pub(crate) fn register_operation_handler(
        &mut self,
        operation: OperationType,
        sender: DynSender<CmdMetaSyncSignal>,
    ) {
        self.op_handler_senders.insert(operation, sender);
    }

    /// Register senders for operation listeners that must be notified when the given operation completes
    pub(crate) fn register_operation_listener(
        &mut self,
        operation: OperationType,
        sender: DynSender<CmdMetaSyncSignal>,
    ) {
        self.op_listener_senders
            .entry(operation)
            .or_default()
            .push(sender);
    }

    /// Send sync signal to all registered listeners for the given operation
    pub(crate) async fn sync_listener(
        &mut self,
        operation: OperationType,
    ) -> Result<(), ChannelError> {
        let Some(senders) = self.op_listener_senders.get_mut(&operation) else {
            return Ok(());
        };
        for sender in senders {
            sender.send(()).await?;
        }
        Ok(())
    }

    /// Send sync signal to the operation handler actor registered for the given operation
    pub(crate) async fn sync(&mut self, operation: OperationType) -> Result<(), ChannelError> {
        if let Some(sender) = self.op_handler_senders.get_mut(&operation) {
            sender.send(()).await?;
        };
        Ok(())
    }

    /// Send sync signal to all registered operation handler actors
    pub(crate) async fn sync_all(&mut self) -> Result<(), ChannelError> {
        for sender in self.op_handler_senders.values_mut() {
            sender.send(()).await?;
        }
        Ok(())
    }
}
