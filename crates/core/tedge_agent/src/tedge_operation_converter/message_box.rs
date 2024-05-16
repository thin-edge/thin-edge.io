use async_trait::async_trait;
use log::error;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::LoggingSender;
use tedge_actors::Message;
use tedge_actors::Sender;
use tedge_api::commands::Command;
use tedge_api::commands::CommandPayload;
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
    pub fn add_operation_manager<Payload>(&mut self, sender: DynSender<Command<Payload>>)
    where
        Payload: CommandPayload + Message + DeserializeOwned,
    {
        let operation = Payload::operation_type().to_string();
        let sender = LoggingSender::new(
            format!("{operation} actor"),
            CommandSender { sender }.into(),
        );
        self.senders.insert(operation, sender.into());
    }

    /// Register where to send commands of a given type
    pub fn register_operation_handler(
        &mut self,
        operation: OperationName,
        sender: DynSender<GenericCommandState>,
    ) {
        self.senders.insert(operation, sender);
    }

    /// List the operations for which a builtin handler has been registered
    pub fn capabilities(&self) -> Vec<&OperationName> {
        self.senders.keys().collect()
    }
}

struct CommandSender<Payload> {
    sender: DynSender<Command<Payload>>,
}

#[async_trait]
impl<Payload> Sender<GenericCommandState> for CommandSender<Payload>
where
    Payload: CommandPayload + Message + DeserializeOwned,
{
    async fn send(&mut self, message: GenericCommandState) -> Result<(), ChannelError> {
        let Some(target) = message.target().and_then(|t| t.parse().ok()) else {
            error!("Not an operation topic: {}", message.topic.as_ref());
            return Ok(());
        };
        let Some(cmd_id) = message.cmd_id() else {
            error!("Not an operation topic: {}", message.topic.as_ref());
            return Ok(());
        };

        match Command::<Payload>::try_from_json(target, cmd_id, message.payload) {
            Ok(cmd) => {
                self.sender.send(cmd).await?;
            }
            Err(err) => error!(
                "Incorrect {operation} request payload: {err}",
                operation = Payload::operation_type()
            ),
        }
        Ok(())
    }
}

impl<Payload: Message> Clone for CommandSender<Payload> {
    fn clone(&self) -> Self {
        CommandSender {
            sender: self.sender.sender_clone(),
        }
    }
}
