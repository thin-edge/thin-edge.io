use crate::{Message, Reactor, Recipient, RuntimeError};
use async_trait::async_trait;
use std::fmt::Debug;

/// A source of messages
#[async_trait]
pub trait Producer<M: Message> {
    /// Produce the messages of this source sending them to the given recipient
    async fn produce_messages(self, output: impl Recipient<M>) -> Result<(), RuntimeError>;
}

/// Akin to `/dev/null`
///
/// - Produce no messages
/// - Consume any message, silently dropping them
#[derive(Clone, Debug)]
pub struct DevNull;

#[async_trait]
impl<M: Message> Producer<M> for DevNull {
    async fn produce_messages(self, _output: impl Recipient<M>) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[async_trait]
impl<M: Message> Recipient<M> for DevNull {
    async fn send_message(&mut self, _message: M) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[async_trait]
impl<I: Message, O: Message> Reactor<I, O> for DevNull {
    async fn react(
        &mut self,
        _message: I,
        _output: &mut impl Recipient<O>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }
}
