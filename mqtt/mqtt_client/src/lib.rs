//! A library to connect the local MQTT bus, publish messages and subscribe topics.
//!
//! ```
//! use mqtt_client::{Config,Message,Topic};
//!
//! #[tokio::main]
//! async fn main (){
//!     let mqtt = Config::default().connect("temperature").await.unwrap();
//!     let c8y_msg = Topic::new("c8y/s/us").unwrap();
//!     mqtt.publish(Message::new(&c8y_msg, "211,23")).await.unwrap();
//!     mqtt.disconnect().await.unwrap();
//! }
//! ```

use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::Outgoing;
use rumqttc::Packet::Publish;
use rumqttc::QoS;
use tokio::sync::broadcast;

/// A connection to the local MQTT bus.
///
/// The host and port are implied: a connection can only be open on the localhost, port 1883.
///
/// ```
/// use mqtt_client::{Config,Message,Topic};
///
/// #[tokio::main]
/// async fn main () {
///     let mqtt = Config::default().connect("temperature").await.unwrap();
///     let c8y_msg = Topic::new("c8y/s/us").unwrap();
///     mqtt.publish(Message::new(&c8y_msg, "211,23")).await.unwrap();
///     mqtt.disconnect().await.unwrap();
/// }
/// ```
pub struct Client {
    pub name: String,
    mqtt_client: rumqttc::AsyncClient,
    message_sender: broadcast::Sender<Message>,
    error_sender: broadcast::Sender<Error>,
}

impl Client {
    /// Open a connection to the local MQTT bus, using the given name to register an MQTT session.
    ///
    /// Reusing the same session name on each connection allows a client
    /// to have its subscriptions persisted by the broker
    /// so messages sent while the client is disconnected
    /// will be resent on its re-connection.
    ///
    /// ```
    /// use mqtt_client::{Config,Client,Topic};
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
        mqtt_options.set_clean_session(false);

        let in_flight = 10;
        let (mqtt_client, event_loop) = rumqttc::AsyncClient::new(mqtt_options, in_flight);
        let (message_sender, _) = broadcast::channel(in_flight);
        let (error_sender, _) = broadcast::channel(in_flight);

        tokio::spawn(Client::bg_process(
            event_loop,
            message_sender.clone(),
            error_sender.clone(),
        ));

        Ok(Client {
            name,
            mqtt_client,
            message_sender,
            error_sender,
        })
    }

    /// Publish a message on the local MQTT bus.
    ///
    pub async fn publish(&self, message: Message) -> Result<(), Error> {
        let qos = QoS::AtLeastOnce;
        let retain = false;
        self.mqtt_client
            .publish(&message.topic.name, qos, retain, message.payload)
            .await
            .map_err(Error::client_error)
    }

    /// Subscribe to the messages published on the given topics
    pub async fn subscribe(&self, filter: TopicFilter) -> Result<MessageStream, Error> {
        let qos = QoS::AtLeastOnce;
        self.mqtt_client
            .subscribe(&filter.pattern, qos)
            .await
            .map_err(Error::client_error)?;

        Ok(MessageStream::new(
            filter,
            self.message_sender.subscribe(),
            self.error_sender.clone(),
        ))
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
    pub fn subscribe_errors(&self) -> ErrorStream {
        ErrorStream::new(self.error_sender.subscribe())
    }

    /// Disconnect the client and drop it.
    pub async fn disconnect(self) -> Result<(), Error> {
        self.mqtt_client
            .disconnect()
            .await
            .map_err(Error::client_error)
    }

    /// Process all the MQTT events
    /// - broadcasting the incoming messages to the message sender,
    /// - broadcasting the errors to the error sender.
    async fn bg_process(
        mut event_loop: rumqttc::EventLoop,
        message_sender: broadcast::Sender<Message>,
        error_sender: broadcast::Sender<Error>,
    ) {
        loop {
            match event_loop.poll().await {
                Err(err) => {
                    // The message sender can only fail if there is no listener
                    // So we simply discard any sender error
                    let _ = error_sender.send(Error::connection_error(err));
                }
                Ok(Event::Incoming(Publish(msg))) => {
                    // The message sender can only fail if there is no listener
                    // So we simply discard any sender error
                    let _ = message_sender.send(Message {
                        topic: Topic::incoming(&msg.topic),
                        payload: msg.payload.to_vec(),
                    });
                }
                Ok(Event::Incoming(Incoming::Disconnect))
                | Ok(Event::Outgoing(Outgoing::Disconnect)) => {
                    break;
                }
                _ => (),
            }
        }
    }
}

/// Configuration of the connection to the MQTT broker.
pub struct Config {
    pub host: String,
    pub port: u16,
}

/// By default a client connects the local MQTT broker.
impl Default for Config {
    fn default() -> Self {
        Config {
            host: String::from("localhost"),
            port: 1883,
        }
    }
}

impl Config {
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
        }
    }
}

/// An MQTT topic filter
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TopicFilter {
    pub pattern: String,
}

impl TopicFilter {
    /// Check if the pattern is valid and build a new topic filter.
    pub fn new(pattern: &str) -> Result<TopicFilter, Error> {
        let pattern = String::from(pattern);
        if rumqttc::valid_filter(&pattern) {
            Ok(TopicFilter { pattern })
        } else {
            Err(Error::InvalidFilter { pattern })
        }
    }

    /// Check if the given topic matches this filter pattern.
    fn accept(&self, topic: &Topic) -> bool {
        rumqttc::matches(&topic.name, &self.pattern)
    }
}

/// A message to be sent to or received from MQTT
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Message {
    pub topic: Topic,
    pub payload: Vec<u8>,
}

impl Message {
    pub fn new<B>(topic: &Topic, payload: B) -> Message
    where
        B: Into<Vec<u8>>,
    {
        Message {
            topic: topic.clone(),
            payload: payload.into(),
        }
    }
}

/// A stream of messages matching a topic filter
pub struct MessageStream {
    filter: TopicFilter,
    receiver: broadcast::Receiver<Message>,
    error_sender: broadcast::Sender<Error>,
}

impl MessageStream {
    fn new(
        filter: TopicFilter,
        receiver: broadcast::Receiver<Message>,
        error_sender: broadcast::Sender<Error>,
    ) -> MessageStream {
        MessageStream {
            filter,
            receiver,
            error_sender,
        }
    }

    /// Return the next message received from MQTT for that subscription, if any.
    /// - Return None when the MQTT connection has been closed
    /// - If too many messages have been received since the previous call to `next()`
    ///   these messages are discarded, and an `Error::MessagesSkipped{lag}`
    ///   is broadcast to the error stream.
    pub async fn next(&mut self) -> Option<Message> {
        loop {
            match self.receiver.recv().await {
                Ok(message) if self.filter.accept(&message.topic) => return Some(message),
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(lag)) => {
                    // The error is forwarded to the client.
                    // The error sender can only fail if there is no listener
                    // So we simply discard any sender error
                    let _ = self.error_sender.send(Error::messages_skipped(lag));
                    continue;
                }
            }
        }
    }
}

/// A stream of errors received asynchronously by the MQTT connection
pub struct ErrorStream {
    receiver: broadcast::Receiver<Error>,
}

impl ErrorStream {
    fn new(receiver: broadcast::Receiver<Error>) -> ErrorStream {
        ErrorStream { receiver }
    }

    /// Return the next MQTT error, if any
    /// - Return None when the MQTT connection has been closed
    /// - Return an `Error::ErrorsSkipped{lag}`
    ///   if too many errors have been received since the previous call to `next()`
    ///   and have been discarded.
    pub async fn next(&mut self) -> Option<Error> {
        match self.receiver.recv().await {
            Ok(error) => Some(error),
            Err(broadcast::error::RecvError::Closed) => None,
            Err(broadcast::error::RecvError::Lagged(lag)) => Some(Error::errors_skipped(lag)),
        }
    }
}

/// An MQTT related error
#[derive(thiserror::Error, Debug, Clone, Eq, PartialEq)]
pub enum Error {
    #[error("Invalid topic name: {name:?}")]
    InvalidTopic { name: String },

    #[error("Invalid topic filter: {pattern:?}")]
    InvalidFilter { pattern: String },

    #[error("MQTT client error: {0}")]
    ClientError(String),

    #[error("MQTT connection error: {0}")]
    ConnectionError(String),

    #[error("The receiver lagged too far behind : {lag:?} messages skipped")]
    MessagesSkipped { lag: u64 },

    #[error("The error lagged too far behind : {lag:?} errors skipped")]
    ErrorsSkipped { lag: u64 },
}

impl Error {
    fn client_error(err: rumqttc::ClientError) -> Error {
        Error::ClientError(format!("{}", err))
    }

    fn connection_error(err: rumqttc::ConnectionError) -> Error {
        Error::ConnectionError(format!("{}", err))
    }

    fn messages_skipped(lag: u64) -> Error {
        Error::MessagesSkipped { lag }
    }
    fn errors_skipped(lag: u64) -> Error {
        Error::ErrorsSkipped { lag }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_valid_topic() {
        assert_eq!(Topic::new("temp").err(), None);
        assert_eq!(Topic::new("temp/device-12").err(), None);
    }

    #[test]
    fn check_invalid_topic() {
        assert_eq!(Topic::new("/temp/+").ok(), None);
        assert_eq!(Topic::new("/temp/#").ok(), None);
    }

    #[test]
    fn check_valid_topic_filter() {
        assert_eq!(TopicFilter::new("a/b/c").err(), None);
        assert_eq!(TopicFilter::new("a/b/#").err(), None);
        assert_eq!(TopicFilter::new("a/b/+").err(), None);
        assert_eq!(TopicFilter::new("a/+/b").err(), None);
    }

    #[test]
    fn check_invalid_topic_filter() {
        assert_eq!(TopicFilter::new("").ok(), None);
        assert_eq!(TopicFilter::new("/a/#/b").ok(), None);
        assert_eq!(TopicFilter::new("/a/#/+").ok(), None);
    }
}
