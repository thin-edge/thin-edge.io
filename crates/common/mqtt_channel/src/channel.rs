use crate::MqttError;
use crate::MqttMessage;
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::SinkExt;
use rumqttc::AsyncClient;
use rumqttc::Publish;
use tracing::error;

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
    queue: mpsc::UnboundedReceiver<Publish>,
    source: AsyncClient,
}

impl SubChannelReceiver {
    pub fn new(queue: mpsc::UnboundedReceiver<Publish>, source: AsyncClient) -> Self {
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
    max_packet_size: Option<usize>,
    queue: mpsc::UnboundedSender<Publish>,
    source: AsyncClient,
}

impl SubChannelSender {
    pub fn new(queue: mpsc::UnboundedSender<Publish>, source: AsyncClient) -> Self {
        SubChannelSender {
            max_packet_size: None,
            queue,
            source,
        }
    }

    pub fn with_max_packet_size(self, size: usize) -> Self {
        Self {
            max_packet_size: Some(size),
            ..self
        }
    }

    pub async fn send(&mut self, message: Publish) {
        if self
            .max_packet_size
            .is_some_and(|max| max < message.payload.len())
        {
            error!(target: "MQTT", "Dropping message received on topic {} with payload size {} that exceeds the maximum packet size of {}",
                message.topic, message.payload.len(), self.max_packet_size.unwrap());
            let _ = self.source.ack(&message).await;
        } else {
            // Errors on send are ignored: it just means the client has closed the receiving channel.
            let _ = self.queue.send(message).await;
        }
    }

    pub async fn close(&mut self) {
        let _ = self.queue.close().await;
    }
}
