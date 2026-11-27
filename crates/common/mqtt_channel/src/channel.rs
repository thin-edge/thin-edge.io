use crate::MqttError;
use crate::MqttMessage;
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::SinkExt;
use rumqttc::AsyncClient;
use rumqttc::Publish;
use rumqttc::QoS;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tracing::error;
use tracing::warn;

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
///
/// QoS 0 messages are dropped if more than max_inflight messages are pending.
pub struct SubChannelReceiver {
    queue: Arc<MessageChannel>,
    source: AsyncClient,
}

impl SubChannelReceiver {
    pub async fn next(&mut self) -> Option<MqttMessage> {
        let message = self.queue.recv().await?;
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
    sender: Arc<MessageChannel>,
    source: AsyncClient,
}

impl SubChannelSender {
    pub fn new(source: AsyncClient) -> Self {
        let sender = MessageChannel::new();
        SubChannelSender {
            max_packet_size: None,
            sender: Arc::new(sender),
            source,
        }
    }

    pub fn with_max_inflight(self, max_inflight: usize) -> Self {
        match Arc::try_unwrap(self.sender) {
            Ok(sender) => Self {
                sender: Arc::new(sender.with_max_inflight(max_inflight)),
                ..self
            },
            Err(sender) => {
                warn!(target: "MQTT", "Cannot update input message queue size after connection");
                Self { sender, ..self }
            }
        }
    }

    pub fn with_max_packet_size(self, size: usize) -> Self {
        Self {
            max_packet_size: Some(size),
            ..self
        }
    }

    pub fn receiver(&self) -> SubChannelReceiver {
        SubChannelReceiver {
            queue: self.sender.clone(),
            source: self.source.clone(),
        }
    }

    pub async fn send(&self, message: Publish) {
        if self
            .max_packet_size
            .is_some_and(|max| max < message.payload.len())
        {
            error!(target: "MQTT", "Dropping message received on topic {} with payload size {} that exceeds the maximum packet size of {}",
                message.topic, message.payload.len(), self.max_packet_size.unwrap());
            let _ = self.source.ack(&message).await;
        } else {
            self.sender.send(message).await;
        }
    }

    pub async fn clear(&self) {
        self.sender.clear().await;
    }

    pub async fn close(&self) {
        self.sender.close().await;
    }
}

struct MessageChannel {
    max_inflight: usize,
    queue: Mutex<MessageQueue>,
    notify: Notify,
}

struct MessageQueue {
    is_closed: bool,
    messages: VecDeque<Publish>,
}

impl MessageChannel {
    pub fn new() -> Self {
        let messages = VecDeque::new();
        let queue = MessageQueue {
            messages,
            is_closed: false,
        };
        MessageChannel {
            max_inflight: 1024,
            queue: Mutex::new(queue),
            notify: Notify::new(),
        }
    }

    pub fn with_max_inflight(self, max_inflight: usize) -> Self {
        MessageChannel {
            max_inflight,
            ..self
        }
    }

    pub async fn send(&self, message: Publish) {
        let mut queue = self.queue.lock().await;
        assert!(!queue.is_closed, "Sending messages on a closed channel");

        if queue.messages.len() >= self.max_inflight && message.qos == QoS::AtMostOnce {
            warn!(target: "MQTT", "Dropping QoS 0 message, exceeding {} inflight message count", self.max_inflight);
        } else {
            queue.messages.push_back(message);
            self.notify.notify_one();
        }
    }

    pub async fn recv(&self) -> Option<Publish> {
        loop {
            let message = self.queue.lock().await.pop_front()?;
            if message.is_some() {
                return message;
            }

            self.notify.notified().await;
        }
    }

    pub async fn clear(&self) {
        self.queue.lock().await.clear();
    }

    pub async fn close(&self) {
        self.queue.lock().await.close();
    }
}

impl MessageQueue {
    /// Return None when the queue has been closed and no more messages are pending
    /// Return Ok(None) when no message is pending but the queue is still open waiting for more messages
    /// Return Ok(Some(message)) when there is a pending message
    pub fn pop_front(&mut self) -> Option<Option<Publish>> {
        match self.messages.pop_front() {
            None if self.is_closed => None,
            maybe_message => Some(maybe_message),
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    pub fn close(&mut self) {
        self.is_closed = true;
    }
}
