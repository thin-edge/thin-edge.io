use crate::MqttError;
use crate::MqttMessage;
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::SinkExt;
use rumqttc::AsyncClient;
use rumqttc::Publish;

#[async_trait]
pub trait SubChannel {
    /// Get the next message received from MQTT - unless the connection has been closed.
    async fn next(&mut self) -> Option<MqttMessage>;
}

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
impl PubChannel for mpsc::UnboundedSender<MqttMessage> {}

/// A channel used by an MQTT client to receive messages
///
/// Messages are queued when received from the broker,
/// and only acknowledged when consumed by the client.
pub struct SubChannelReceiver {
    queue: mpsc::Receiver<Publish>,
    source: AsyncClient,
}

impl SubChannelReceiver {
    pub fn new(queue: mpsc::Receiver<Publish>, source: AsyncClient) -> Self {
        SubChannelReceiver { queue, source }
    }

    pub async fn next(&mut self) -> Option<MqttMessage> {
        let message = self.queue.recv().await.ok()?;
        let _ = self.source.ack(&message).await;
        Some(message.into())
    }
}

#[async_trait]
impl SubChannel for SubChannelReceiver {
    async fn next(&mut self) -> Option<MqttMessage> {
        SubChannelReceiver::next(self).await
    }
}

#[async_trait]
impl SubChannel for mpsc::Receiver<MqttMessage> {
    async fn next(&mut self) -> Option<MqttMessage> {
        self.recv().await.ok()
    }
}

pub(crate) struct SubChannelSender {
    queue: mpsc::Sender<Publish>,
    source: AsyncClient,
}

impl SubChannelSender {
    pub fn new(queue: mpsc::Sender<Publish>, source: AsyncClient) -> Self {
        SubChannelSender { queue, source }
    }

    pub async fn send(&mut self, message: Publish) {
        let _ = self.queue.send(message).await;
    }
    pub async fn drop(&mut self, message: Publish) {
        let _ = self.source.ack(&message).await;
    }
    pub async fn close(&mut self) {
        let _ = self.queue.close().await;
    }
}
