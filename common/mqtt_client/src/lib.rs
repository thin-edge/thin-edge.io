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
use mockall::automock;
pub use rumqttc::QoS;
use rumqttc::{Event, Incoming, Outgoing, Packet, Publish, Request, StateError};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicIsize, Ordering},
    Arc,
};
use tokio::sync::{broadcast, Notify};

#[automock]
#[async_trait]
pub trait MqttClient: Send + Sync {
    fn subscribe_errors(&self) -> Box<dyn MqttErrorStream>;

    async fn subscribe(
        &self,
        filter: TopicFilter,
    ) -> Result<Box<dyn MqttMessageStream>, MqttClientError>;

    async fn publish(&self, message: Message) -> Result<(), MqttClientError>;
}

#[async_trait]
#[automock]
pub trait MqttMessageStream: Send + Sync {
    async fn next(&mut self) -> Option<Message>;
}

#[async_trait]
#[automock]
pub trait MqttErrorStream: Send + Sync {
    async fn next(&mut self) -> Option<Arc<MqttClientError>>;
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
    error_sender: broadcast::Sender<Arc<MqttClientError>>,
    join_handle: tokio::task::JoinHandle<()>,
    requests_tx: rumqttc::Sender<Request>,
    inflight: Arc<InflightTracking>,
}

/// Tracks the number of inflight / pending publish requests.
#[derive(Debug)]
struct InflightTracking {
    /// Tracks number of pending publish message until they are
    /// known to be sent out by the event loop.
    pending_publish_count: AtomicIsize,
    /// Tracks number of pending puback's (not completed messages of QoS=1).
    pending_puback_count: AtomicIsize,
    /// Tracks number of pending pubcomp's (not completed messages of QoS=2).
    pending_pubcomp_count: AtomicIsize,

    /// Notify on the condition when all requests have completed.
    notify_completed: Notify,
}

impl InflightTracking {
    fn new() -> Self {
        Self {
            pending_publish_count: AtomicIsize::new(0),
            pending_puback_count: AtomicIsize::new(0),
            pending_pubcomp_count: AtomicIsize::new(0),
            notify_completed: Notify::new(),
        }
    }

    fn has_pending(&self) -> bool {
        self.pending_publish_count.load(Ordering::Relaxed) > 0
            || self.pending_puback_count.load(Ordering::Relaxed) > 0
            || self.pending_pubcomp_count.load(Ordering::Relaxed) > 0
    }

    /// Resolves when all pending requests have been completed.
    async fn all_completed(&self) {
        while self.has_pending() {
            // Calling `notify_one()` before `notified().await` is safe, no signal is lost.
            //
            // Nevertheless, we still use a timeout (but a larger one) to be on the safe side and
            // not risk any race condition.
            let _ = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                self.notify_completed.notified(),
            )
            .await;
        }
    }

    fn track_publish_request(&self, qos: QoS) {
        self.pending_publish_count.fetch_add(1, Ordering::Relaxed);
        match qos {
            QoS::AtMostOnce => {}
            QoS::AtLeastOnce => {
                self.pending_puback_count.fetch_add(1, Ordering::Relaxed);
            }
            QoS::ExactlyOnce => {
                self.pending_pubcomp_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn track_publish_request_sentout(&self) {
        self.pending_publish_count.fetch_sub(1, Ordering::Relaxed);
        self.check_completed();
    }

    fn track_publish_qos1_completed(&self) {
        self.pending_puback_count.fetch_sub(1, Ordering::Relaxed);
        self.check_completed();
    }

    fn track_publish_qos2_completed(&self) {
        self.pending_pubcomp_count.fetch_sub(1, Ordering::Relaxed);
        self.check_completed();
    }

    fn check_completed(&self) {
        if !self.has_pending() {
            self.notify_completed.notify_one();
        }
    }
}

// Send a message on a broadcast channel discarding any error.
// The send can only fail if there is no listener.
macro_rules! send_discarding_error {
    ($sender:expr, $msg:expr) => {
        let _ = $sender.send($msg);
    };
}

/// MQTT message id
type MessageId = u16;

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
    pub async fn connect(name: &str, config: &Config) -> Result<Client, MqttClientError> {
        let name = String::from(name);
        let mut mqtt_options = rumqttc::MqttOptions::new(&name, &config.host, config.port);
        mqtt_options.set_clean_session(config.clean_session);
        if let Some(inflight) = config.inflight {
            mqtt_options.set_inflight(inflight);
        }

        if let Some(packet_size) = config.packet_size {
            mqtt_options.set_max_packet_size(packet_size, packet_size);
        }

        let (mqtt_client, eventloop) =
            rumqttc::AsyncClient::new(mqtt_options, config.queue_capacity);
        let requests_tx = eventloop.requests_tx.clone();
        let (message_sender, _) = broadcast::channel(config.queue_capacity);
        let (error_sender, _) = broadcast::channel(config.queue_capacity);

        let inflight = Arc::new(InflightTracking::new());

        let join_handle = tokio::spawn(Client::bg_process(
            eventloop,
            message_sender.clone(),
            error_sender.clone(),
            inflight.clone(),
        ));

        Ok(Client {
            name,
            mqtt_client,
            message_sender,
            error_sender,
            join_handle,
            requests_tx,
            inflight,
        })
    }

    /// Returns the name used by the MQTT client.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Disconnect the client and drop it.
    pub async fn disconnect(self) -> Result<(), MqttClientError> {
        let () = self.mqtt_client.disconnect().await?;
        self.join_handle
            .await
            .map_err(|_| MqttClientError::JoinError)
    }

    /// Returns `true` if there are pending inflight messages.
    pub fn has_pending(&self) -> bool {
        self.inflight.has_pending()
    }

    /// Resolves when all pending requests have been completed.
    pub async fn all_completed(&self) {
        self.inflight.all_completed().await
    }

    /// Process all the MQTT events
    /// - broadcasting the incoming messages to the message sender,
    /// - broadcasting the errors to the error sender.
    async fn bg_process(
        mut event_loop: rumqttc::EventLoop,
        message_sender: broadcast::Sender<Message>,
        error_sender: broadcast::Sender<Arc<MqttClientError>>,
        inflight: Arc<InflightTracking>,
    ) {
        // Delay announcing a QoS=2 message to the client until we have seen a PUBREL.
        let mut pending_received_messages: HashMap<MessageId, Message> = HashMap::new();

        loop {
            match event_loop.poll().await {
                Ok(Event::Incoming(Packet::Publish(msg))) => {
                    match msg.qos {
                        QoS::AtLeastOnce | QoS::AtMostOnce => {
                            send_discarding_error!(message_sender, msg.into());
                        }
                        QoS::ExactlyOnce => {
                            // Do not announce the incoming publish message immediately in case
                            // of QoS=2. Wait for the PUBREL.

                            let _ = pending_received_messages.insert(msg.pkid, msg.into());
                        }
                    }
                }

                Ok(Event::Incoming(Packet::PubRel(pubrel))) => {
                    if let Some(msg) = pending_received_messages.remove(&pubrel.pkid) {
                        assert!(msg.qos == QoS::ExactlyOnce);
                        send_discarding_error!(message_sender, msg);
                    }
                }

                Ok(Event::Outgoing(Outgoing::Publish(_id))) => {
                    inflight.track_publish_request_sentout();
                }

                Ok(Event::Incoming(Packet::PubAck(_))) => {
                    // Reception of PUBACK means that a QoS=1 request completed.
                    inflight.track_publish_qos1_completed();
                }

                Ok(Event::Incoming(Packet::PubComp(_))) => {
                    // Reception of PUBCOMP means that a QoS=2 request completed.
                    inflight.track_publish_qos2_completed();
                }

                Ok(Event::Incoming(Incoming::Disconnect))
                | Ok(Event::Outgoing(Outgoing::Disconnect)) => {
                    break;
                }

                Err(err) => {
                    let delay = match &err {
                        rumqttc::ConnectionError::Io(_) => true,
                        rumqttc::ConnectionError::MqttState(state_error)
                            if matches!(state_error, StateError::Io(_)) =>
                        {
                            true
                        }
                        rumqttc::ConnectionError::MqttState(_) => true,
                        rumqttc::ConnectionError::Mqtt4Bytes(_) => true,
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
    /// This does not wait until the acknowledge is received.
    ///
    async fn publish(&self, message: Message) -> Result<(), MqttClientError> {
        let qos = message.qos;
        let request = Request::Publish(message.into());

        let () = self
            .requests_tx
            .send(request)
            .await
            .map_err(|err| MqttClientError::ClientError(rumqttc::ClientError::Request(err)))?;

        // Track number of pending publish requests.
        self.inflight.track_publish_request(qos);

        Ok(())
    }

    /// Subscribe to the messages published on the given topics
    async fn subscribe(
        &self,
        filter: TopicFilter,
    ) -> Result<Box<dyn MqttMessageStream>, MqttClientError> {
        let qos = filter.qos;
        for pattern in filter.patterns.iter() {
            let () = self.mqtt_client.subscribe(pattern, qos).await?;
        }

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

    /// Max packet size limit for outgoing an incoming packets
    /// If `None` is provided, 10KB size limit is imposed
    pub packet_size: Option<usize>,

    /// Capacity of various internal message queues.
    ///
    /// This is used to decouple both `rumqttc` and our background event loop from
    /// frontend operations.
    ///
    /// Default: `10`.
    pub queue_capacity: usize,

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
            // 256MB by default
            packet_size: Some(268435455),
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

    /// Update queue_capacity.
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
    pub async fn connect(&self, name: &str) -> Result<Client, MqttClientError> {
        Client::connect(name, self).await
    }

    /// Set a max packet size
    pub fn with_packet_size(self, packet_size: usize) -> Self {
        Self {
            packet_size: Some(packet_size),
            ..self
        }
    }
}

/// An MQTT topic
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Topic {
    pub name: String,
}

impl Topic {
    /// Check if the topic name is valid and build a new topic.
    pub fn new(name: &str) -> Result<Topic, MqttClientError> {
        let name = String::from(name);
        if rumqttc::valid_topic(&name) {
            Ok(Topic { name })
        } else {
            Err(MqttClientError::InvalidTopic { name })
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
            patterns: vec![self.name.clone()],
            qos: QoS::AtLeastOnce,
        }
    }
}

/// An MQTT topic filter
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TopicFilter {
    pub patterns: Vec<String>,
    pub qos: QoS,
}

impl TopicFilter {
    /// Check if the pattern is valid and build a new topic filter.
    pub fn new(pattern: &str) -> Result<TopicFilter, MqttClientError> {
        let pattern = String::from(pattern);
        let qos = QoS::AtLeastOnce;
        if rumqttc::valid_filter(&pattern) {
            Ok(TopicFilter {
                patterns: vec![pattern],
                qos,
            })
        } else {
            Err(MqttClientError::InvalidFilter { pattern })
        }
    }

    /// Check if the pattern is valid and at it to this topic filter.
    pub fn add(&mut self, pattern: &str) -> Result<(), MqttClientError> {
        let pattern = String::from(pattern);
        if rumqttc::valid_filter(&pattern) {
            self.patterns.push(pattern);
            Ok(())
        } else {
            Err(MqttClientError::InvalidFilter { pattern })
        }
    }

    /// Check if the given topic matches this filter pattern.
    fn accept(&self, topic: &Topic) -> bool {
        self.patterns
            .iter()
            .any(|pattern| rumqttc::matches(&topic.name, pattern))
    }

    pub fn qos(self, qos: QoS) -> Self {
        Self { qos, ..self }
    }
}

pub type Payload = Vec<u8>;

/// A message to be sent to or received from MQTT.
///
/// NOTE: We never set the `pkid` of the `Publish` message,
/// as this might conflict with ids generated by `rumqttc`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Message {
    pub topic: Topic,
    payload: Payload,
    pub qos: QoS,
    pkid: u16,
    pub retain: bool,
}

impl Message {
    pub fn new<B>(topic: &Topic, payload: B) -> Message
    where
        B: Into<Payload>,
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

    pub fn retain(self) -> Self {
        Self {
            retain: true,
            ..self
        }
    }

    // trims the trailing null char if one exists
    pub fn payload_trimmed(&self) -> &[u8] {
        self.payload
            .strip_suffix(&[0])
            .unwrap_or_else(|| self.payload.as_slice())
    }

    // This function trims the null character at the end of the payload before converting into UTF8
    // Some MQTT messages contain the payload with trailing null char, such payload is invalid payload.
    pub fn payload_str(&self) -> Result<&str, MqttClientError> {
        let payload_trimmed = self.payload_trimmed();
        std::str::from_utf8(payload_trimmed)
            .map_err(|err| new_invalid_utf8_payload(payload_trimmed, err))
    }

    pub fn payload_raw(&self) -> &[u8] {
        &self.payload[..]
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
    error_sender: broadcast::Sender<Arc<MqttClientError>>,
}

impl MessageStream {
    fn new(
        filter: TopicFilter,
        receiver: broadcast::Receiver<Message>,
        error_sender: broadcast::Sender<Arc<MqttClientError>>,
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
                        Arc::new(MqttClientError::MessagesSkipped { lag })
                    );
                    continue;
                }
            }
        }
    }
}

/// A stream of errors received asynchronously by the MQTT connection
pub struct ErrorStream {
    receiver: broadcast::Receiver<Arc<MqttClientError>>,
}

impl ErrorStream {
    fn new(receiver: broadcast::Receiver<Arc<MqttClientError>>) -> ErrorStream {
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
    async fn next(&mut self) -> Option<Arc<MqttClientError>> {
        match self.receiver.recv().await {
            Ok(error) => Some(error),
            Err(broadcast::error::RecvError::Closed) => None,
            Err(broadcast::error::RecvError::Lagged(lag)) => {
                Some(Arc::new(MqttClientError::ErrorsSkipped { lag }))
            }
        }
    }
}

/// An MQTT related error
#[derive(thiserror::Error, Debug)]
pub enum MqttClientError {
    #[error("Invalid topic name: {name:?}")]
    InvalidTopic { name: String },

    #[error("Invalid topic filter: {pattern:?}")]
    InvalidFilter { pattern: String },

    #[error("MQTT client error: {0}")]
    ClientError(#[from] rumqttc::ClientError),

    #[error("Stream error: {0}")]
    StreamError(Box<MqttClientError>),

    #[error("MQTT connection error: {0}")]
    ConnectionError(#[from] rumqttc::ConnectionError),

    #[error("The receiver lagged too far behind : {lag:?} messages skipped")]
    MessagesSkipped { lag: u64 },

    #[error("The error lagged too far behind : {lag:?} errors skipped")]
    ErrorsSkipped { lag: u64 },

    #[error("Broadcast receive error: {0}")]
    BroadcastRecvError(#[from] broadcast::error::RecvError),

    #[error("Join Error")]
    JoinError,

    #[error("Invalid UTF8 payload: {from}: {input_excerpt}...")]
    InvalidUtf8Payload {
        input_excerpt: String,
        from: std::str::Utf8Error,
    },
}

fn new_invalid_utf8_payload(bytes: &[u8], from: std::str::Utf8Error) -> MqttClientError {
    const EXCERPT_LEN: usize = 80;
    let index = from.valid_up_to();
    let input = std::str::from_utf8(&bytes[..index]).unwrap_or("");

    MqttClientError::InvalidUtf8Payload {
        input_excerpt: input_prefix(input, EXCERPT_LEN),
        from,
    }
}

fn input_prefix(input: &str, len: usize) -> String {
    input
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(len)
        .collect()
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
    #[test]
    fn payload_str_with_invalid_utf8_char_in_the_middle() {
        let topic = Topic::new("trimmed").unwrap();
        let message = Message::new(&topic, &b"temperature\xc3\x28"[..]);
        assert_eq!(
            message.payload_str().unwrap_err().to_string(),
            "Invalid UTF8 payload: invalid utf-8 sequence of 1 bytes from index 11: temperature..."
        );
    }
    #[test]
    fn payload_str_with_invalid_utf8_char_in_the_beginning() {
        let topic = Topic::new("trimmed").unwrap();
        let message = Message::new(&topic, &b"\xc3\x28"[..]);
        assert_eq!(
            message.payload_str().unwrap_err().to_string(),
            "Invalid UTF8 payload: invalid utf-8 sequence of 1 bytes from index 0: ..."
        );
    }
}
