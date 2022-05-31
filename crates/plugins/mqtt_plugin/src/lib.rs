/// This is a quick implementation an actor around rumqttc
///
/// This crate cannot be used beyond a POC for many reasons.
/// - This is quick workaround over the fact that rumqttc requires a tokio runtime.
/// - A first attempt has been to use the mqtt_channel crate.
///   ... but this crate inherits the constraint of a tokio runtime.
///   ... and the mqtt_channel API doesn't provide a mean to run the connection in a separate thread over channels.
/// - Hence, this code is a quick adaption of code extracted from the mqtt_channel crate.
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use std::thread::sleep;
use std::time::Duration;
use tedge_actors::{Actor, Message, Recipient, RuntimeError, RuntimeHandler, Task};

#[derive(Clone, Debug)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: String,
}

impl Message for MqttMessage {}

#[derive(Clone, Debug)]
pub struct MqttConfig {
    pub session_name: String,
    pub port: u16,
    pub subscriptions: Vec<String>,
}

pub enum MqttConnection {
    Stopped { mqtt_config: MqttConfig },
    Running { mqtt_con: ConnectionHandler },
}

#[async_trait]
impl Actor for MqttConnection {
    type Config = MqttConfig;
    type Input = MqttMessage;
    type Output = MqttMessage;

    fn try_new(mqtt_config: Self::Config) -> Result<Self, RuntimeError> {
        Ok(MqttConnection::Stopped { mqtt_config })
    }

    async fn start(
        &mut self,
        mut runtime: RuntimeHandler,
        output: Recipient<MqttMessage>,
    ) -> Result<(), RuntimeError> {
        match self {
            MqttConnection::Running { .. } => Ok(()),
            MqttConnection::Stopped { mqtt_config } => {
                let (message_sender, message_receiver) = mpsc::channel(16);

                runtime
                    .spawn(Connection {
                        config: mqtt_config.clone(),
                        message_receiver,
                        message_sender: output,
                    })
                    .await?;

                *self = MqttConnection::Running {
                    mqtt_con: ConnectionHandler { message_sender },
                };

                Ok(())
            }
        }
    }

    async fn react(
        &mut self,
        message: MqttMessage,
        _runtime: &mut RuntimeHandler,
        _output: &mut Recipient<MqttMessage>,
    ) -> Result<(), RuntimeError> {
        match self {
            MqttConnection::Stopped { .. } => Ok(()),
            MqttConnection::Running { mqtt_con } => {
                Ok(mqtt_con.message_sender.send(message).await?)
            }
        }
    }
}

use rumqttc::{
    AsyncClient, ConnectionError, Event, Incoming, Outgoing, Packet, Publish, QoS, SubscribeFilter,
};

pub struct ConnectionHandler {
    message_sender: mpsc::Sender<MqttMessage>,
}

struct Connection {
    config: MqttConfig,
    message_receiver: mpsc::Receiver<MqttMessage>,
    message_sender: Recipient<MqttMessage>,
}

#[async_trait]
impl Task for Connection {
    async fn run(self: Box<Self>) -> Result<(), RuntimeError> {
        std::thread::spawn(|| {
            let _ = self.run_bg();
        });
        Ok(())
    }
}

impl Connection {
    // rumqttc requires a tokio runtime
    #[tokio::main]
    async fn run_bg(self) -> Result<(), RuntimeError> {
        let config = self.config;
        let mut message_sender = self.message_sender;
        let mut message_receiver = self.message_receiver;

        let mqtt_options = rumqttc::MqttOptions::new(config.session_name, "localhost", config.port);
        let (mqtt_client, mut event_loop) = AsyncClient::new(mqtt_options, 1024);

        // open the connection
        loop {
            match event_loop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(ack))) => {
                    if let Some(err) = MqttError::maybe_connection_error(&ack) {
                        return Err(err.into());
                    };
                    let subscriptions = config
                        .subscriptions
                        .iter()
                        .map(|topic| SubscribeFilter {
                            path: topic.clone(),
                            qos: QoS::AtLeastOnce,
                        })
                        .collect::<Vec<SubscribeFilter>>();
                    if subscriptions.is_empty() {
                        break;
                    }
                    let _ = mqtt_client.subscribe_many(subscriptions).await;
                }

                Ok(Event::Incoming(Packet::SubAck(ack))) => {
                    if let Some(err) = MqttError::maybe_subscription_error(&ack) {
                        return Err(err.into());
                    };
                    break;
                }

                Ok(Event::Incoming(Packet::Publish(msg))) => {
                    // Messages can be received before a sub ack
                    // Errors on send are ignored: it just means the client has closed the receiving channel.
                    let _ = message_sender.send_message(msg.into()).await;
                }

                Err(err) => {
                    let should_delay = Connection::pause_on_error(&err);

                    eprintln!("MQTT Error: {:?}", err);

                    if should_delay {
                        Connection::do_pause().await;
                    }
                }
                _ => (),
            }
        }

        // publish outgoing messages
        tokio::spawn(async move {
            while let Some(message) = message_receiver.next().await {
                if let Err(err) = mqtt_client
                    .publish(message.topic, QoS::AtLeastOnce, false, message.payload)
                    .await
                {
                    eprintln!("MQTT Error: {:?}", err);
                }
            }
        });

        // forward incoming messages
        loop {
            let event = event_loop.poll().await;
            match event {
                Ok(Event::Incoming(Packet::Publish(msg))) => {
                    // Errors on send are ignored: it just means the client has closed the receiving channel.
                    // One has to continue the loop though, because rumqttc relies on this polling.
                    let _ = message_sender.send_message(msg.into()).await;
                }

                Ok(Event::Incoming(Incoming::Disconnect))
                | Ok(Event::Outgoing(Outgoing::Disconnect)) => {
                    // The connection has been closed
                    break;
                }

                Err(err) => {
                    eprintln!("MQTT Error: {:?}", err);
                    if Connection::pause_on_error(&err) {
                        Connection::do_pause().await;
                    }
                }
                _ => (),
            }
        }

        Ok(())
    }

    fn pause_on_error(err: &ConnectionError) -> bool {
        match &err {
            rumqttc::ConnectionError::Io(_)
            | rumqttc::ConnectionError::MqttState(_)
            | rumqttc::ConnectionError::Mqtt4Bytes(_) => true,
            _ => false,
        }
    }

    async fn do_pause() {
        sleep(Duration::from_secs(1));
    }
}

impl From<MqttMessage> for Publish {
    fn from(val: MqttMessage) -> Self {
        Publish::new(&val.topic, QoS::AtLeastOnce, val.payload)
    }
}

impl From<Publish> for MqttMessage {
    fn from(msg: Publish) -> Self {
        let Publish { topic, payload, .. } = msg;

        MqttMessage {
            topic,
            payload: String::from(std::str::from_utf8(&payload).expect("utf8")),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum MqttError {
    #[error("Invalid topic name: {name:?}")]
    InvalidTopic { name: String },

    #[error("Invalid topic filter: {pattern:?}")]
    InvalidFilter { pattern: String },

    #[error("Invalid session: a session name must be provided")]
    InvalidSessionConfig,

    #[error("MQTT client error: {0}")]
    ClientError(#[from] rumqttc::ClientError),

    #[error("MQTT connection error: {0}")]
    ConnectionError(#[from] rumqttc::ConnectionError),

    #[error("MQTT connection rejected: {0:?}")]
    ConnectionRejected(rumqttc::ConnectReturnCode),

    #[error("MQTT subscription failure")]
    // The MQTT specs are mysterious on the possible cause of such a failure
    SubscriptionFailure,

    #[error("Invalid UTF8 payload: {from}: {input_excerpt}...")]
    InvalidUtf8Payload {
        input_excerpt: String,
        from: std::str::Utf8Error,
    },

    #[error(
        "The read channel of the connection has been closed and no more messages can be received"
    )]
    ReadOnClosedConnection,

    #[error(
        "The send channel of the connection has been closed and no more messages can be published"
    )]
    SendOnClosedConnection,
}

impl Into<RuntimeError> for MqttError {
    fn into(self) -> RuntimeError {
        RuntimeError::ConfigError
    }
}

impl MqttError {
    pub fn maybe_connection_error(ack: &rumqttc::ConnAck) -> Option<MqttError> {
        match ack.code {
            rumqttc::ConnectReturnCode::Success => None,
            err => Some(MqttError::ConnectionRejected(err)),
        }
    }

    pub fn maybe_subscription_error(ack: &rumqttc::SubAck) -> Option<MqttError> {
        for code in ack.return_codes.iter() {
            if let rumqttc::SubscribeReasonCode::Failure = code {
                return Some(MqttError::SubscriptionFailure);
            }
        }
        None
    }

    pub fn new_invalid_utf8_payload(bytes: &[u8], from: std::str::Utf8Error) -> MqttError {
        const EXCERPT_LEN: usize = 80;
        let index = from.valid_up_to();
        let input = std::str::from_utf8(&bytes[..index]).unwrap_or("");

        MqttError::InvalidUtf8Payload {
            input_excerpt: MqttError::input_prefix(input, EXCERPT_LEN),
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
}

#[cfg(test)]
mod tests;
