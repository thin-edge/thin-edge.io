use core::fmt;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::Outgoing;
use rumqttc::Packet::Publish;
use rumqttc::QoS;
use tokio::sync::broadcast;
use tokio_compat_02::FutureExt;

pub struct Client {
    pub name: String,
    mqtt_client: rumqttc::AsyncClient,
    message_sender: broadcast::Sender<Message>,
    error_sender: broadcast::Sender<Error>,
}

impl Client {
    pub async fn connect(name: &str) -> Result<Client, Error> {
        let name = String::from(name);
        let mut mqtt_options = rumqttc::MqttOptions::new(&name, "localhost", 1883);
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

    pub async fn publish(&self, message: Message) -> Result<(), Error> {
        let qos = QoS::AtLeastOnce;
        let retain = false;
        self.mqtt_client
            .publish(&message.topic.name, qos, retain, message.payload)
            .compat() // required because rumqtt uses tokio 0.2, not 0.3 as we do
            .await
            .map_err(Error::client_error)
    }

    pub async fn subscribe(&self, filter: TopicFilter) -> Result<MessageStream, Error> {
        let qos = QoS::AtLeastOnce;
        self.mqtt_client
            .subscribe(&filter.pattern, qos)
            .compat() // required because rumqtt uses tokio 0.2, not 0.3 as we do
            .await
            .map_err(Error::client_error)?;

        Ok(MessageStream::new(
            filter,
            self.message_sender.subscribe(),
            self.error_sender.clone(),
        ))
    }

    pub fn subscribe_errors(&self) -> ErrorStream {
        ErrorStream::new(self.error_sender.subscribe())
    }

    pub async fn disconnect(self) -> Result<(), Error> {
        self.mqtt_client
            .disconnect()
            .compat() // required because rumqtt uses tokio 0.2, not 0.3 as we do
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
            match event_loop.poll().compat().await {
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

    pub fn filter(&self) -> TopicFilter {
        TopicFilter {
            pattern: self.name.clone(),
        }
    }
}

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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Error {
    InvalidTopic { name: String },
    InvalidFilter { pattern: String },
    ClientError(String),
    ConnectionError(String),
    MessagesSkipped { lag: u64 },
    ErrorsSkipped { lag: u64 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::InvalidTopic { ref name } => write!(f, "Invalid topic name: {}", name),
            Error::InvalidFilter { ref pattern } => write!(f, "Invalid topic filter: {}", pattern),
            Error::ClientError(ref err) => write!(f, "MQTT client error: {}", err),
            Error::ConnectionError(ref err) => write!(f, "MQTT connection error: {}", err),
            Error::MessagesSkipped { lag } => write!(
                f,
                "The receiver lagged too far behind : {} messages skipped",
                lag
            ),
            Error::ErrorsSkipped { lag } => write!(
                f,
                "The error receiver lagged too far behind : {} errors skipped",
                lag
            ),
        }
    }
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
