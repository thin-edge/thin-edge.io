use anyhow::anyhow;
use anyhow::bail;
use bytes::Bytes;
use core::panic;
use futures::future::pending;
use rumqttc::ClientError;
use rumqttc::ConnectionError;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::Outgoing;
use rumqttc::PubAck;
use rumqttc::Publish;
use rumqttc::Request;

use rumqttc::QoS;
use rumqttc::SubscribeFilter;
use std::collections::VecDeque;
use std::sync::atomic::AtomicU16;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::sync::Mutex as TokioMutex;

use crate::MqttAck;
use crate::MqttClient;
use crate::MqttEvents;

#[macro_export]
macro_rules! inc {
    (connack) => {
        Ok(Event::Incoming(Incoming::ConnAck(ConnAck {
            session_present: true,
            code: ConnectReturnCode::Success,
        })))
    };
    (suback) => {
        Ok(Event::Incoming(Incoming::SubAck(SubAck {
            pkid: 1,
            return_codes: vec![SubscribeReasonCode::Success(QoS::AtLeastOnce)],
        })))
    };
    (publish($msg:expr)) => {
        Ok(Event::Incoming(Incoming::Publish($msg.clone())))
    };
    (puback($pkid:expr)) => {
        Ok(Event::Incoming(Incoming::PubAck(PubAck { pkid: $pkid })))
    };
    (network_error) => {
        Err(ConnectionError::NetworkTimeout)
    };
}

#[macro_export]
macro_rules! out {
    (publish($pkid:expr)) => {
        Ok(Event::Outgoing(Outgoing::Publish($pkid)))
    };
}

pub type EventRes = Result<Event, rumqttc::ConnectionError>;

#[async_trait::async_trait]
/// Encapsulates the logic for waiting for all messages to finish processing
pub trait AllProcessed {
    async fn all_processed(&self) -> anyhow::Result<()>;
}

#[derive(Default, Debug, Clone)]
/// A fixed stream of events
pub struct FixedEventStream {
    events: Arc<Mutex<VecDeque<EventRes>>>,
}

impl FixedEventStream {
    fn next_event(&self) -> Option<EventRes> {
        self.events.lock().unwrap().pop_front()
    }
}

impl<I: Into<VecDeque<EventRes>>> From<I> for FixedEventStream {
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
            event
        } else {
            pending().await
        }
    }

    fn take_pending(&mut self) -> VecDeque<Request> {
        <_>::default()
    }

    fn set_pending(&mut self, _requests: Vec<Request>) {}
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

#[derive(Clone)]
/// An [MqttClient] implementation that blocks on [MqttClient::subscribe_many]
///
/// This is used to verify we continue making progress, polling the event loop,
/// when it may be full of requests.
pub struct BlockingSubscribeClient;

#[async_trait::async_trait]
impl MqttAck for BlockingSubscribeClient {
    async fn ack(&self, _publish: &Publish) -> Result<(), rumqttc::ClientError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl MqttClient for BlockingSubscribeClient {
    async fn subscribe_many(&self, _: Vec<SubscribeFilter>) -> Result<(), ClientError> {
        pending().await
    }

    async fn publish(&self, _: String, _: QoS, _: bool, _: Bytes) -> Result<(), ClientError> {
        unimplemented!()
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

/// Generates a client/event-loop pair
pub fn channel_client_and_events() -> (ChannelClient, ChannelEvents) {
    let (tx, rx) = mpsc::channel(10);
    let client = ChannelClient::new(tx);
    let events = ChannelEvents::new(rx, client.clone());
    (client, events)
}

#[derive(Clone)]
/// A stream of publishes driven by [ChannelClient]
pub struct ChannelEvents(Arc<TokioMutex<ChannelEventsInner>>);

struct ChannelEventsInner {
    connected: bool,
    rx: mpsc::Receiver<Publish>,
    count: u16,
    pkid: u16,
    in_flight: Option<u16>,
    client: ChannelClient,
}

impl ChannelEvents {
    pub fn new(rx: mpsc::Receiver<Publish>, client: ChannelClient) -> Self {
        Self(Arc::new(TokioMutex::new(ChannelEventsInner {
            connected: false,
            rx,
            count: 0,
            pkid: 0,
            in_flight: None,
            client,
        })))
    }

    pub async fn message_count(&self) -> u16 {
        self.0.lock().await.count
    }
}

#[async_trait::async_trait]
impl AllProcessed for ChannelEvents {
    async fn all_processed(&self) -> anyhow::Result<()> {
        loop {
            let inner = self.0.lock().await;
            if inner.client.count_in_progress.load(Ordering::SeqCst) == 0 && inner.rx.is_empty() {
                return Ok(());
            }
            std::mem::drop(inner);
            tokio::task::yield_now().await;
        }
    }
}

#[async_trait::async_trait]
impl MqttEvents for ChannelEvents {
    async fn poll(&mut self) -> Result<Event, ConnectionError> {
        let mut inner = self.0.lock().await;
        if !inner.connected {
            while inner.rx.len() < inner.rx.capacity() {
                drop(inner);
                tokio::task::yield_now().await;
                inner = self.0.lock().await;
            }
            inner.connected = true;
        }
        if let Some(pkid) = inner.in_flight.take() {
            inc!(puback(pkid))
        } else {
            // We need to loop over `try_recv` so we don't hold the lock
            //
            // This allows `all_processed` to detect when we are blocked
            // because we have consumed all the messages.
            loop {
                match inner.rx.try_recv() {
                    Ok(_publish) => {
                        inner.count += 1;
                        inner.pkid = inner.pkid % inner.rx.capacity() as u16 + 1;
                        inner.in_flight = Some(inner.pkid);
                        break out!(publish(inner.pkid));
                    }
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        panic!("disconnected")
                    }
                    Err(mpsc::error::TryRecvError::Empty) => {
                        drop(inner);
                        tokio::task::yield_now().await;
                        inner = self.0.lock().await;
                    }
                }
            }
        }
    }

    fn take_pending(&mut self) -> VecDeque<Request> {
        unimplemented!()
    }

    fn set_pending(&mut self, _requests: Vec<Request>) {
        unimplemented!()
    }
}

#[derive(Clone)]
pub struct ChannelClient {
    tx: mpsc::Sender<Publish>,
    count_in_progress: Arc<AtomicU16>,
}

impl ChannelClient {
    fn new(tx: mpsc::Sender<Publish>) -> Self {
        Self {
            tx,
            count_in_progress: <_>::default(),
        }
    }
}

#[async_trait::async_trait]
impl MqttAck for ChannelClient {
    async fn ack(&self, _publish: &Publish) -> Result<(), rumqttc::ClientError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl MqttClient for ChannelClient {
    async fn subscribe_many(&self, _: Vec<SubscribeFilter>) -> Result<(), ClientError> {
        Ok(())
    }

    async fn publish(
        &self,
        topic: String,
        qos: QoS,
        _: bool,
        payload: Bytes,
    ) -> Result<(), ClientError> {
        self.count_in_progress.fetch_add(1, Ordering::SeqCst);
        self.tx
            .send(Publish::new(topic, qos, payload))
            .await
            .unwrap();
        self.count_in_progress.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
}
