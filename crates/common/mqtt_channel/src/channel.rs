use crate::Message;
use crate::MqttError;
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::{Sink, SinkExt};
use futures::{Stream, StreamExt};

#[async_trait]
pub trait SubChannel: StreamExt<Item = Message> + Unpin + Send {}

#[async_trait]
pub trait ErrChannel: StreamExt<Item = MqttError> + Unpin + Send {}

#[async_trait]
pub trait PubChannel: SinkExt<Message> + Unpin + Send {
    /// Publish a message - unless the pub channel has been closed.
    async fn publish(&mut self, message: Message) -> Result<(), MqttError> {
        Ok(self
            .send(message)
            .await
            .map_err(|_| MqttError::SendOnClosedConnection)?)
    }
}

#[async_trait]
impl SubChannel for mpsc::UnboundedReceiver<Message> {}

#[async_trait]
impl ErrChannel for mpsc::UnboundedReceiver<MqttError> {}

#[async_trait]
impl PubChannel for mpsc::UnboundedSender<Message> {}
