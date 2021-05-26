//! A library to connect the local MQTT bus, publish messages and subscribe topics.
//!
//! ```no_run
//! use mqtt_client::{MqttClient,Config,Message,Topic};
//!
//! #[tokio::main]
//! async fn main (){
//!     let mqtt = Config::default().connect("temperature").await.unwrap();
//!     let c8y_msg = Topic::new("c8y/s/us").unwrap();
//!     mqtt.publish(Message::new(&c8y_msg, "211,23")).await.unwrap();
//!     mqtt.disconnect().await.unwrap();
//! }
//! ```
#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use async_trait::async_trait;
use futures::future::Future;
use mockall::automock;
pub use rumqttc::QoS;
use rumqttc::{Event, Incoming, Outgoing, Packet, Publish, Request, StateError};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot};

#[automock]
#[async_trait]
pub trait MqttClient: Send + Sync {
    fn subscribe_errors(&self) -> Box<dyn MqttErrorStream>;

    async fn subscribe(&self, filter: TopicFilter) -> Result<Box<dyn MqttMessageStream>, Error>;

    async fn publish(&self, message: Message) -> Result<MessageId, Error>;
}

#[async_trait]
#[automock]
pub trait MqttMessageStream: Send + Sync {
    async fn next(&mut self) -> Option<Message>;
}

#[async_trait]
#[automock]
pub trait MqttErrorStream: Send + Sync {
    async fn next(&mut self) -> Option<Arc<Error>>;
}

/// A connection to the local MQTT bus.
///
/// The host and port are implied: a connection can only be open on the localhost, port 1883.
///
/// ```no_run
/// use mqtt_client::{Config,Message,MqttClient,Topic};
///
/// #[tokio::main]
/// async fn main () {
///     let mut mqtt = Config::default().connect("temperature").await.unwrap();
///     let c8y_msg = Topic::new("c8y/s/us").unwrap();
///     mqtt.publish(Message::new(&c8y_msg, "211,23")).await.unwrap();
///     mqtt.disconnect().await.unwrap();
/// }
/// ```
#[derive(Debug)]
pub struct Client {
    name: String,
    mqtt_client: rumqttc::AsyncClient,
    message_sender: broadcast::Sender<Message>,
    error_sender: broadcast::Sender<Arc<Error>>,
    ack_event_sender: broadcast::Sender<AckEvent>,
    join_handle: tokio::task::JoinHandle<()>,
    requests_tx: rumqttc::Sender<Request>,
}

// Send a message on a broadcast channel discarding any error.
// The send can only fail if there is no listener.
macro_rules! send_discarding_error {
    ($sender:expr, $msg:expr) => {
        let _ = $sender.send($msg);
    };
}

/// MQTT message id
pub type MessageId = u16;

/// `AckEvent` contains all events related to communicating acknowledgement of messages between our
/// background event loop (see `Client::bg_process`) and frontend operations like
/// `Client#publish_with_ack`.
///
/// This is required so that `Client#publish_with_ack` can wait for a `PubAck` (QoS=1) or `PubComp`
/// (QoS=2) before returning to the caller. To be able to wait for an acknowledgement, we first
/// need to obtain the `pkid` of the message. For this purpose we introduced
/// `Request::PublishWithNotify` to the underlying `rumqttc` library, which takes a
/// `oneshot::channel` that it resolves once the `pkid` is generated.
///
#[derive(Debug, Copy, Clone)]
enum AckEvent {
    PubAckReceived(MessageId),
    PubCompReceived(MessageId),
}

impl Client {
    /// Open a connection to the local MQTT bus, using the given name to register an MQTT session.
    ///
    /// Reusing the same session name on each connection allows a client
    /// to have its subscriptions persisted by the broker
    /// so messages sent while the client is disconnected
    /// will be resent on its re-connection.
    ///
    /// ```no_run
    /// use mqtt_client::{Config,Client,MqttClient,Topic};
    ///
    /// #[tokio::main]
    /// async fn main () {
    ///     let c8y_cmd = Topic::new("c8y/s/ds").unwrap();
    ///     let config = Config::default();
    ///
    ///     let mqtt = Client::connect("temperature", &config).await.unwrap();
    ///     let mut commands = mqtt.subscribe(c8y_cmd.filter()).await.unwrap();
    ///     // process some commands and disconnect
    ///     mqtt.disconnect().await.unwrap();
    ///
    ///     // wait a while and reconnect
    ///     let mqtt = Client::connect("temperature", &config).await.unwrap();
    ///     let mut commands = mqtt.subscribe(c8y_cmd.filter()).await.unwrap();
    ///     // process the messages even those sent during the pause
    /// }
    /// ```
    pub async fn connect(name: &str, config: &Config) -> Result<Client, Error> {
        let name = String::from(name);
        let mut mqtt_options = rumqttc::MqttOptions::new(&name, &config.host, config.port);
        mqtt_options.set_clean_session(config.clean_session);
        if let Some(inflight) = config.inflight {
            mqtt_options.set_inflight(inflight);
        }

        let (mqtt_client, eventloop) =
            rumqttc::AsyncClient::new(mqtt_options, config.queue_capacity);
        let requests_tx = eventloop.requests_tx.clone();
        let (message_sender, _) = broadcast::channel(config.queue_capacity);
        let (error_sender, _) = broadcast::channel(config.queue_capacity);
        let (ack_event_sender, _) = broadcast::channel(config.queue_capacity);

        let join_handle = tokio::spawn(Client::bg_process(
            eventloop,
            message_sender.clone(),
            error_sender.clone(),
            ack_event_sender.clone(),
        ));

        Ok(Client {
            name,
            mqtt_client,
            message_sender,
            error_sender,
            ack_event_sender,
            join_handle,
            requests_tx,
        })
    }

    /// Returns the name used by the MQTT client.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Publish a message on the local MQTT bus.
    ///
    /// Supports awaiting the acknowledge.
    ///
    /// Upon success a `Future` is returned that resolves once the publish is acknowledged (only in
    /// case QoS=1 or QoS=2).
    ///
    /// Example:
    ///
    /// ```no_run
    /// #[tokio::main]
    /// async fn main() {
    ///     use mqtt_client::*;
    ///     let topic = Topic::new("c8y/s/us").unwrap();
    ///     let mqtt = Config::default().connect("temperature").await.unwrap();
    ///     let ack = mqtt.publish_with_ack(Message::new(&topic, "211,23")).await.unwrap();
    ///     let () = ack.await.unwrap();
    /// }
    /// ```
    pub async fn publish_with_ack(
        &self,
        message: Message,
    ) -> Result<impl Future<Output = Result<(), Error>>, Error> {
        let qos = message.qos;

        // Subscribe to the acknowledgement events. In case of QoS=1 or QoS=2 we
        // need to wait for an PubAck / PubComp to confirm that the publish has succeeded.
        let mut ack_events = AckEventStream::new(self.ack_event_sender.subscribe());

        let pkid = self.publish(message).await?;

        Ok(async move {
            match qos {
                QoS::AtMostOnce => {}
                QoS::AtLeastOnce => {
                    ack_events.wait_for_pub_ack_received(pkid).await?;
                }
                QoS::ExactlyOnce => {
                    ack_events.wait_for_pub_comp_received(pkid).await?;
                }
            }
            Ok(())
        })
    }

    /// Disconnect the client and drop it.
    pub async fn disconnect(self) -> Result<(), Error> {
        let () = self.mqtt_client.disconnect().await?;
        self.join_handle.await.map_err(|_| Error::JoinError)
    }

    /// Process all the MQTT events
    /// - broadcasting the incoming messages to the message sender,
    /// - broadcasting the errors to the error sender.
    async fn bg_process(
        mut event_loop: rumqttc::EventLoop,
        message_sender: broadcast::Sender<Message>,
        error_sender: broadcast::Sender<Arc<Error>>,
        ack_event_sender: broadcast::Sender<AckEvent>,
    ) {
        let mut pending: HashMap<MessageId, Message> = HashMap::new();

        loop {
            match event_loop.poll().await {
                Ok(Event::Incoming(Packet::Publish(msg))) => {
                    match msg.qos {
                        QoS::AtLeastOnce | QoS::AtMostOnce => {
                            send_discarding_error!(message_sender, msg.into());
                        }
                        QoS::ExactlyOnce => {
                            // Do not annouce the incoming publish message immediatly in case
                            // of QoS=2. Wait for the `PubRel`.
                            //
                            // TODO: Avoid buffering the message here by
                            // moving that part of the code into the `rumqttc` library.
                            let _ = pending.insert(msg.pkid, msg.into());
                        }
                    }
                }

                Ok(Event::Incoming(Packet::PubRel(pubrel))) => {
                    if let Some(msg) = pending.remove(&pubrel.pkid) {
                        assert!(msg.qos == QoS::ExactlyOnce);
                        // TODO: `rumqttc` library with cargo feature="v5" has a
                        // field `PubRel#reason`. Check that for `rumqttc::PubRelReason::Success`
                        // and only notify in that case.
                        send_discarding_error!(message_sender, msg);
                    }
                }

                Ok(Event::Incoming(Packet::PubAck(ack))) => {
                    send_discarding_error!(ack_event_sender, AckEvent::PubAckReceived(ack.pkid));
                }

                Ok(Event::Incoming(Packet::PubComp(ack))) => {
                    send_discarding_error!(ack_event_sender, AckEvent::PubCompReceived(ack.pkid));
                }

                Ok(Event::Incoming(Incoming::Disconnect))
                | Ok(Event::Outgoing(Outgoing::Disconnect)) => {
                    break;
                }

                Err(err) => {
                    let delay = match &err {
                        rumqttc::ConnectionError::Io(io_err)
                            if matches!(io_err.kind(), std::io::ErrorKind::ConnectionRefused) =>
                        {
                            true
                        }

                        rumqttc::ConnectionError::MqttState(state_error)
                            if matches!(state_error, StateError::Io(_)) =>
                        {
                            true
                        }

                        _ => false,
                    };

                    send_discarding_error!(error_sender, Arc::new(err.into()));

                    if delay {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
                _ => (),
            }
        }
    }
}

#[async_trait]
impl MqttClient for Client {
    /// Publish a message on the local MQTT bus.
    ///
    /// This does not wait for until the acknowledge is received.
    ///
    /// Upon success, this returns the `pkid` of the published message.
    ///
    async fn publish(&self, message: Message) -> Result<MessageId, Error> {
        let (sender, receiver) = oneshot::channel();
        let request = Request::PublishWithNotify {
            publish: message.into(),
            notify: sender,
        };

        let () = self
            .requests_tx
            .send(request)
            .await
            .map_err(|err| Error::ClientError(rumqttc::ClientError::Request(err)))?;

        // Wait for the confirmation from the `rumqttc` backend that a `pkid` has been assigned
        // to the message. We need the `pkid` in order to wait for the corresponding
        // acknowledgement message.
        let pkid: MessageId = receiver.await?;

        Ok(pkid)
    }

    /// Subscribe to the messages published on the given topics
    async fn subscribe(&self, filter: TopicFilter) -> Result<Box<dyn MqttMessageStream>, Error> {
        let () = self
            .mqtt_client
            .subscribe(&filter.pattern, filter.qos)
            .await?;

        Ok(Box::new(MessageStream::new(
            filter,
            self.message_sender.subscribe(),
            self.error_sender.clone(),
        )))
    }

    /// Subscribe to the errors raised asynchronously.
    ///
    /// These errors include connection errors.
    /// When the system fails to establish an MQTT connection with the local broker,
    /// or when the current connection is lost, the system tries in the background to reconnect.
    /// the client. Each connection error is forwarded to the `ErrorStream` returned by `subscribe_errors()`.
    ///
    /// These errors also include internal client errors.
    /// Such errors are related to unread messages on the subscription channels.
    /// If a client subscribes to a topic but fails to consume the received messages,
    /// these messages will be dropped and an `Error::MessagesSkipped{lag}` will be published
    /// in the `ErrorStream` returned by `subscribe_errors()`.
    ///
    /// If the `ErrorStream` itself is not read fast enough (i.e there are too many in-flight error messages)
    /// these error messages will be dropped and replaced by an `Error::ErrorsSkipped{lag}`.
    fn subscribe_errors(&self) -> Box<dyn MqttErrorStream> {
        Box::new(ErrorStream::new(self.error_sender.subscribe()))
    }
}

/// Configuration of the connection to the MQTT broker.
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    /// maximum number of outgoing inflight messages (pending acknowledgement).
    ///
    /// If `None` is provided, the default setting of `rumqttc` is used.
    pub inflight: Option<u16>,

    /// Capacity of various internal message queues.
    ///
    /// This is used to decouple both `rumqttc` and our background event loop from
    /// frontend operations.
    ///
    /// Default: `10`.
    queue_capacity: usize,

    /// Clean the MQTT session upon connect if set to `true`.
    ///
    /// Default: `false`.
    clean_session: bool,
}

/// By default a client connects the local MQTT broker.
impl Default for Config {
    fn default() -> Self {
        Config {
            host: String::from("localhost"),
            port: 1883,
            inflight: None,
            queue_capacity: 10,
            clean_session: false,
        }
    }
}

impl Config {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            ..Config::default()
        }
    }

    /// Set a custom port
    pub fn with_port(self, port: u16) -> Self {
        Self { port, ..self }
    }

    /// Update queue_capcity.
    pub fn queue_capacity(self, queue_capacity: usize) -> Self {
        Self {
            queue_capacity,
            ..self
        }
    }

    /// Enable `clean_session`.
    pub fn clean_session(self) -> Self {
        Self {
            clean_session: true,
            ..self
        }
    }

    /// Use this config to connect a MQTT client
    pub async fn connect(&self, name: &str) -> Result<Client, Error> {
        Client::connect(name, self).await
    }
}

/// An MQTT topic
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Topic {
    pub name: String,
}

impl Topic {
    /// Check if the topic name is valid and build a new topic.
    pub fn new(name: &str) -> Result<Topic, Error> {
        let name = String::from(name);
        if rumqttc::valid_topic(&name) {
            Ok(Topic { name })
        } else {
            Err(Error::InvalidTopic { name })
        }
    }

    /// Build a new topic, assuming the name is valid since received from mqtt.
    fn incoming(name: &str) -> Topic {
        let name = String::from(name);
        Topic { name }
    }

    /// Build a topic filter filtering only that topic
    pub fn filter(&self) -> TopicFilter {
        TopicFilter {
            pattern: self.name.clone(),
            qos: QoS::AtLeastOnce,
        }
    }
}

/// An MQTT topic filter
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TopicFilter {
    pub pattern: String,
    pub qos: QoS,
}

impl TopicFilter {
    /// Check if the pattern is valid and build a new topic filter.
    pub fn new(pattern: &str) -> Result<TopicFilter, Error> {
        let pattern = String::from(pattern);
        let qos = QoS::AtLeastOnce;
        if rumqttc::valid_filter(&pattern) {
            Ok(TopicFilter { pattern, qos })
        } else {
            Err(Error::InvalidFilter { pattern })
        }
    }

    /// Check if the given topic matches this filter pattern.
    fn accept(&self, topic: &Topic) -> bool {
        rumqttc::matches(&topic.name, &self.pattern)
    }

    pub fn qos(self, qos: QoS) -> Self {
        Self { qos, ..self }
    }
}

/// A message to be sent to or received from MQTT.
///
/// NOTE: We never set the `pkid` of the `Publish` message,
/// as this might conflict with ids generated by `rumqttc`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Message {
    pub topic: Topic,
    payload: Vec<u8>,
    pub qos: QoS,
    pkid: u16,
    retain: bool,
}

impl Message {
    pub fn new<B>(topic: &Topic, payload: B) -> Message
    where
        B: Into<Vec<u8>>,
    {
        Message {
            topic: topic.clone(),
            payload: payload.into(),
            qos: QoS::AtLeastOnce,
            pkid: 0,
            retain: false,
        }
    }

    pub fn qos(self, qos: QoS) -> Self {
        Self { qos, ..self }
    }

    /// trimming the trailing null char
    pub fn payload_trimmed(&self) -> &[u8] {
        self.payload
            .strip_suffix(&[0])
            .unwrap_or(self.payload.as_slice())
    }
}

impl From<Message> for Publish {
    fn from(val: Message) -> Self {
        let mut publish = Publish::new(&val.topic.name, val.qos, val.payload);
        publish.retain = val.retain;
        publish
    }
}

impl From<Publish> for Message {
    fn from(msg: Publish) -> Self {
        let Publish {
            topic,
            payload,
            qos,
            pkid,
            retain,
            ..
        } = msg;

        Message {
            topic: Topic::incoming(&topic),
            payload: payload.to_vec(),
            qos,
            pkid,
            retain,
        }
    }
}

/// A stream of messages matching a topic filter
pub struct MessageStream {
    filter: TopicFilter,
    receiver: broadcast::Receiver<Message>,
    error_sender: broadcast::Sender<Arc<Error>>,
}

impl MessageStream {
    fn new(
        filter: TopicFilter,
        receiver: broadcast::Receiver<Message>,
        error_sender: broadcast::Sender<Arc<Error>>,
    ) -> MessageStream {
        MessageStream {
            filter,
            receiver,
            error_sender,
        }
    }
}

#[async_trait]
impl MqttMessageStream for MessageStream {
    /// Return the next message received from MQTT for that subscription, if any.
    /// - Return None when the MQTT connection has been closed
    /// - If too many messages have been received since the previous call to `next()`
    ///   these messages are discarded, and an `Error::MessagesSkipped{lag}`
    ///   is broadcast to the error stream.
    async fn next(&mut self) -> Option<Message> {
        loop {
            match self.receiver.recv().await {
                Ok(message) if self.filter.accept(&message.topic) => return Some(message),
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(lag)) => {
                    // Forward the error to the client.
                    send_discarding_error!(
                        self.error_sender,
                        Arc::new(Error::MessagesSkipped { lag })
                    );
                    continue;
                }
            }
        }
    }
}

/// A stream of errors received asynchronously by the MQTT connection
pub struct ErrorStream {
    receiver: broadcast::Receiver<Arc<Error>>,
}

impl ErrorStream {
    fn new(receiver: broadcast::Receiver<Arc<Error>>) -> ErrorStream {
        ErrorStream { receiver }
    }
}

#[async_trait]
impl MqttErrorStream for ErrorStream {
    /// Return the next MQTT error, if any
    /// - Return None when the MQTT connection has been closed
    /// - Return an `Error::ErrorsSkipped{lag}`
    ///   if too many errors have been received since the previous call to `next()`
    ///   and have been discarded.
    async fn next(&mut self) -> Option<Arc<Error>> {
        match self.receiver.recv().await {
            Ok(error) => Some(error),
            Err(broadcast::error::RecvError::Closed) => None,
            Err(broadcast::error::RecvError::Lagged(lag)) => {
                Some(Arc::new(Error::ErrorsSkipped { lag }))
            }
        }
    }
}

/// A stream of `AckEvent`s received asynchronously from the background event loop.
struct AckEventStream {
    receiver: broadcast::Receiver<AckEvent>,
}

impl AckEventStream {
    fn new(receiver: broadcast::Receiver<AckEvent>) -> Self {
        Self { receiver }
    }

    /// Waits for the next `AckEvent::PubAckReceived` event with the specified `pkid`.
    async fn wait_for_pub_ack_received(&mut self, pkid: MessageId) -> Result<(), Error> {
        loop {
            match self.receiver.recv().await? {
                AckEvent::PubAckReceived(recv_pkid) if recv_pkid == pkid => return Ok(()),
                _ => {}
            }
        }
    }

    /// Waits for the next `AckEvent::PubCompReceived` event with the specified `pkid`.
    async fn wait_for_pub_comp_received(&mut self, pkid: MessageId) -> Result<(), Error> {
        loop {
            match self.receiver.recv().await? {
                AckEvent::PubCompReceived(recv_pkid) if recv_pkid == pkid => return Ok(()),
                _ => {}
            }
        }
    }
}

/// An MQTT related error
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid topic name: {name:?}")]
    InvalidTopic { name: String },

    #[error("Invalid topic filter: {pattern:?}")]
    InvalidFilter { pattern: String },

    #[error("MQTT client error: {0}")]
    ClientError(#[from] rumqttc::ClientError),

    #[error("Stream error: {0}")]
    StreamError(Box<Error>),

    #[error("MQTT connection error: {0}")]
    ConnectionError(#[from] rumqttc::ConnectionError),

    #[error("The receiver lagged too far behind : {lag:?} messages skipped")]
    MessagesSkipped { lag: u64 },

    #[error("The error lagged too far behind : {lag:?} errors skipped")]
    ErrorsSkipped { lag: u64 },

    #[error("Broadcast receive error: {0}")]
    BroadcastRecvError(#[from] broadcast::error::RecvError),

    #[error("Oneshot channel receive error: {0}")]
    OneshotRecvError(#[from] tokio::sync::oneshot::error::RecvError),

    #[error("Join Error")]
    JoinError,
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn check_valid_topic() {
        assert!(Topic::new("temp").is_ok());
        assert!(Topic::new("temp/device-12").is_ok());
    }

    #[test]
    fn check_invalid_topic() {
        assert!(Topic::new("/temp/+").is_err());
        assert!(Topic::new("/temp/#").is_err());
    }

    #[test]
    fn check_valid_topic_filter() {
        assert!(TopicFilter::new("a/b/c").is_ok());
        assert!(TopicFilter::new("a/b/#").is_ok());
        assert!(TopicFilter::new("a/b/+").is_ok());
        assert!(TopicFilter::new("a/+/b").is_ok());
    }

    #[test]
    fn check_invalid_topic_filter() {
        assert!(TopicFilter::new("").is_err());
        assert!(TopicFilter::new("/a/#/b").is_err());
        assert!(TopicFilter::new("/a/#/+").is_err());
    }

    #[test]
    fn check_null_terminated_messages() {
        let topic = Topic::new("trimmed").unwrap();
        let message = Message::new(&topic, &b"123\0"[..]);

        assert_eq!(message.payload_trimmed(), b"123");
    }

    #[test]
    fn payload_trimmed_removes_only_last_null_char() {
        let topic = Topic::new("trimmed").unwrap();
        let message = Message::new(&topic, &b"123\0\0"[..]);

        assert_eq!(message.payload_trimmed(), b"123\0");
    }

    #[test]
    fn check_empty_messages() {
        let topic = Topic::new("trimmed").unwrap();
        let message = Message::new(&topic, &b""[..]);

        assert_eq!(message.payload_trimmed(), b"");
    }

    #[test]
    fn check_non_null_terminated_messages() {
        let topic = Topic::new("trimmed").unwrap();
        let message = Message::new(&topic, &b"123"[..]);

        assert_eq!(message.payload_trimmed(), b"123");
    }
}
