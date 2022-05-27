use crate::{Message, Recipient, RuntimeError, Sender};
use async_trait::async_trait;
use std::fmt::Debug;

/// A source of messages
#[async_trait]
pub trait Producer<M: Message> {
    /// Produce the messages of this source sending them to the given recipient
    async fn produce_messages(self, output: Recipient<M>) -> Result<(), RuntimeError>;
}

/// Akin to `/dev/null`
///
/// - Produce no messages
/// - Consume any message, silently dropping them
#[derive(Clone, Debug)]
pub struct DevNull;

#[async_trait]
impl<M: Message> Producer<M> for DevNull {
    async fn produce_messages(self, _output: Recipient<M>) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[async_trait]
impl<M: Message> Sender<M> for DevNull {
    async fn send_message(&mut self, _message: M) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn clone(&self) -> Recipient<M> {
        Box::new(DevNull)
    }
}

impl<M: Message> Into<Recipient<M>> for DevNull {
    fn into(self) -> Recipient<M> {
        Box::new(self)
    }
}
