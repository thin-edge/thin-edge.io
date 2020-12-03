use core::fmt;
use rumqttc::Event::Incoming;
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

    pub async fn subscribe(&self, topic: &Topic) -> Result<MessageStream, Error> {
        let qos = QoS::AtLeastOnce;
        self.mqtt_client
            .subscribe(&topic.name, qos)
            .compat() // required because rumqtt uses tokio 0.2, not 0.3 as we do
            .await
            .map_err(Error::client_error)?;

        Ok(MessageStream::new(
            topic.clone(),
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

    async fn bg_process(
        mut event_loop: rumqttc::EventLoop,
        message_sender: broadcast::Sender<Message>,
        error_sender: broadcast::Sender<Error>,
    ) {
        loop {
            match event_loop.poll().compat().await {
                Err(err) => {
                    // The sender can only fail if there is no listener
                    // So we simply discard any sender error
                    let _ = error_sender.send(Error::connection_error(err));
                }
                Ok(Incoming(Publish(msg))) => {
                    // The sender can only fail if there is no listener
                    // So we simply discard any sender error
                    let _ = message_sender.send(Message {
                        topic: Topic::new(&msg.topic),
                        payload: msg.payload.to_vec(),
                    });
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
    pub fn new(name: &str) -> Topic {
        let name = String::from(name);
        Topic { name }
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
    filter: Topic,
    receiver: broadcast::Receiver<Message>,
    error_sender: broadcast::Sender<Error>,
}

impl MessageStream {
    fn new(
        filter: Topic,
        receiver: broadcast::Receiver<Message>,
        error_sender: broadcast::Sender<Error>,
    ) -> MessageStream {
        MessageStream {
            filter,
            receiver,
            error_sender,
        }
    }

    fn accept(&self, message: &Message) -> bool {
        rumqttc::matches(&message.topic.name, &self.filter.name)
    }

    pub async fn next(&mut self) -> Option<Message> {
        loop {
            match self.receiver.recv().await {
                Err(broadcast::error::RecvError::Closed) => return None,
                Ok(message) if self.accept(&message) => return Some(message),
                Ok(_) => continue,
                Err(err) => {
                    // The sender can only fail if there is no listener
                    // So we simply discard any sender error
                    let _ = self.error_sender.send(Error::stream_error(err));
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

    pub async fn next(&mut self) -> Option<Error> {
        loop {
            match self.receiver.recv().await {
                Err(broadcast::error::RecvError::Closed) => return None,
                Ok(error) => return Some(error),
                Err(broadcast::error::RecvError::Lagged(lag)) => {
                    eprintln!(
                        "ERROR the error stream is not consumed fast enough: lag of {} messages",
                        lag
                    );
                    continue;
                }
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Error {
    ClientError(String),
    ConnectionError(String),
    StreamError(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::ClientError(ref err) => write!(f, "MQTT client error: {}", err),
            Error::ConnectionError(ref err) => write!(f, "MQTT connection error: {}", err),
            Error::StreamError(ref err) => write!(f, "Stream error: {}", err),
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

    fn stream_error(err: broadcast::error::RecvError) -> Error {
        Error::ConnectionError(format!("{}", err))
    }
}
