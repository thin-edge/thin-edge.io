use anyhow::anyhow;
use anyhow::bail;
use bytes::Bytes;
use core::panic;
use futures::channel::oneshot;
use futures::future::pending;
use rumqttc::ClientError;
use rumqttc::ConnectionError;
use rumqttc::Event;
use rumqttc::Publish;
use rumqttc::QoS;
use rumqttc::SubscribeFilter;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::Mutex as TokioMutex;

use crate::MqttAck;
use crate::MqttClient;
use crate::MqttEvents;

#[macro_export]
macro_rules! inc {
    (connack) => {
        Event::Incoming(Incoming::ConnAck(ConnAck {
            session_present: true,
            code: ConnectReturnCode::Success,
        }))
    };
    (publish($msg:ident)) => {
        Event::Incoming(Incoming::Publish($msg.clone()))
    };
    (puback($pkid:literal)) => {
        Event::Incoming(Incoming::PubAck(PubAck { pkid: $pkid }))
    };
}

#[macro_export]
macro_rules! out {
    (publish($pkid:literal)) => {
        Event::Outgoing(Outgoing::Publish($pkid))
    };
}

#[derive(Default, Debug, Clone)]
/// A fixed stream of events
pub struct FixedEventStream {
    events: Arc<Mutex<VecDeque<Event>>>,
}

pub struct EventsPolled;

#[derive(Clone)]
/// An [MqttClient] implementation that blocks on [MqttClient::subscribe_many]
///
/// This is used to verify that [rumqttc::EventLoop::poll] is called
pub struct BlockingSubscribeClient {
    pub rx: Arc<TokioMutex<Option<oneshot::Receiver<EventsPolled>>>>,
}

impl BlockingSubscribeClient {
    pub fn new(rx: oneshot::Receiver<EventsPolled>) -> Self {
        Self {
            rx: Arc::new(TokioMutex::new(Some(rx))),
        }
    }
}

#[derive(Clone)]
/// Designed to be used with [BlockingSubscribeClient]
pub struct UnblockingEventStream {
    pub events: FixedEventStream,
    pub tx: Arc<TokioMutex<Option<oneshot::Sender<EventsPolled>>>>,
}

impl UnblockingEventStream {
    pub fn new(events: impl Into<FixedEventStream>, tx: oneshot::Sender<EventsPolled>) -> Self {
        Self {
            events: events.into(),
            tx: Arc::new(TokioMutex::new(Some(tx))),
        }
    }
}

#[async_trait::async_trait]
impl AllProcessed for UnblockingEventStream {
    async fn all_processed(&self) -> anyhow::Result<()> {
        self.events.all_processed().await
    }
}

#[derive(Clone)]
/// A client that panics on every operation
pub struct PanickingClient;

#[async_trait::async_trait]
impl AllProcessed for PanickingClient {
    async fn all_processed(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait AllProcessed {
    async fn all_processed(&self) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
impl MqttClient for PanickingClient {
    async fn subscribe_many(&self, _: Vec<SubscribeFilter>) -> Result<(), ClientError> {
        panic!("Called `subscribe_many` on `PanickingClient`")
    }

    async fn publish(&self, _: String, _: QoS, _: bool, _: Bytes) -> Result<(), ClientError> {
        panic!("Called `publish` on `PanickingClient`")
    }
}

#[async_trait::async_trait]
impl MqttAck for PanickingClient {
    async fn ack(&self, _publish: &Publish) -> Result<(), rumqttc::ClientError> {
        panic!("Called `ack` on `PanickingClient`")
    }
}

#[async_trait::async_trait]
impl MqttEvents for UnblockingEventStream {
    async fn poll(&mut self) -> Result<Event, ConnectionError> {
        if let Some(event) = self.events.next_event() {
            Ok(event)
        } else {
            if let Some(tx) = self.tx.lock().await.take() {
                tx.send(EventsPolled).ok().unwrap();
            }
            pending().await
        }
    }
}

#[async_trait::async_trait]
impl MqttAck for BlockingSubscribeClient {
    async fn ack(&self, _publish: &Publish) -> Result<(), rumqttc::ClientError> {
        unimplemented!()
    }
}

#[async_trait::async_trait]
impl MqttClient for BlockingSubscribeClient {
    async fn subscribe_many(&self, _: Vec<SubscribeFilter>) -> Result<(), ClientError> {
        if let Some(rx) = self.rx.lock().await.take() {
            rx.await.unwrap();
        }
        Ok(())
    }

    async fn publish(&self, _: String, _: QoS, _: bool, _: Bytes) -> Result<(), ClientError> {
        unimplemented!()
    }
}

impl FixedEventStream {
    fn next_event(&self) -> Option<Event> {
        self.events.lock().unwrap().pop_front()
    }
}

#[async_trait::async_trait]
impl AllProcessed for FixedEventStream {
    async fn all_processed(&self) -> anyhow::Result<()> {
        let timeout = Duration::from_secs(5);
        let start = Instant::now();
        while !self.events.lock().unwrap().is_empty() {
            if start.elapsed() > timeout {
                bail!("Timed out waiting for event emitter to be fully consumed. Unconsumed events were {:?}", self.events.lock().unwrap())
            }

            tokio::task::yield_now().await;
        }
        Ok(())
    }
}

impl<I: Into<VecDeque<Event>>> From<I> for FixedEventStream {
    fn from(value: I) -> Self {
        Self {
            events: Arc::new(Mutex::new(value.into())),
        }
    }
}

#[async_trait::async_trait]
impl MqttEvents for FixedEventStream {
    async fn poll(&mut self) -> Result<Event, ConnectionError> {
        if let Some(event) = self.next_event() {
            Ok(event)
        } else {
            pending().await
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    SubscribeMany(Vec<SubscribeFilter>),
    Ack(Publish),
    Publish(Publish),
}

#[derive(Default, Debug, Clone)]
pub struct ActionLogger {
    log: Arc<Mutex<VecDeque<Action>>>,
}

impl ActionLogger {
    fn log(&self, action: Action) {
        self.log.lock().unwrap().push_back(action);
    }

    pub fn next_action(&self) -> anyhow::Result<Action> {
        let next_message = self.log.lock().unwrap().pop_front();
        next_message.ok_or(anyhow!("Expected client to be interacted with. Did you forget to call `bridge.<type>_client.all_processed().await`?"))
    }
}

#[async_trait::async_trait]
impl MqttAck for ActionLogger {
    async fn ack(&self, publish: &Publish) -> Result<(), rumqttc::ClientError> {
        self.log(Action::Ack(publish.clone()));
        Ok(())
    }
}
#[async_trait::async_trait]
impl MqttClient for ActionLogger {
    async fn subscribe_many(&self, topics: Vec<SubscribeFilter>) -> Result<(), ClientError> {
        self.log(Action::SubscribeMany(topics));
        Ok(())
    }

    async fn publish(
        &self,
        topic: String,
        qos: QoS,
        retain: bool,
        payload: Bytes,
    ) -> Result<(), ClientError> {
        let mut publish = Publish::new(topic, qos, payload);
        publish.retain = retain;
        self.log(Action::Publish(publish));
        Ok(())
    }
}
