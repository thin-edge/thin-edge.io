use crate::MqttError;
use crate::MqttMessage;
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::SinkExt;
use futures::StreamExt;

#[async_trait]
pub trait SubChannel: StreamExt<Item = MqttMessage> + Unpin + Send {}

#[async_trait]
pub trait ErrChannel: StreamExt<Item = MqttError> + Unpin + Send {}

#[async_trait]
pub trait PubChannel: SinkExt<MqttMessage> + Unpin + Send {
    /// Publish a message - unless the pub channel has been closed.
    async fn publish(&mut self, message: MqttMessage) -> Result<(), MqttError> {
        Ok(self
            .send(message)
            .await
            .map_err(|_| MqttError::SendOnClosedConnection)?)
    }
}

#[async_trait]
impl SubChannel for mpsc::UnboundedReceiver<MqttMessage> {}

#[async_trait]
impl ErrChannel for mpsc::UnboundedReceiver<MqttError> {}

#[async_trait]
impl PubChannel for mpsc::UnboundedSender<MqttMessage> {}
