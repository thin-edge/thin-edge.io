use crate::command::{BuildCommand, Command};
use crate::utils::signals;
use futures::future::FutureExt;
use mqtt_client::{Client, Config, Message, MessageStream, QoS, Topic, TopicFilter};
use std::process;
use std::time::Duration;
use structopt::StructOpt;
use tokio::{io::AsyncWriteExt, select};

const DEFAULT_HOST: &str = "localhost";
const DEFAULT_PORT: u16 = 1883;
const PUB_CLIENT_PREFIX: &str = "tedge-pub";
const SUB_CLIENT_PREFIX: &str = "tedge-sub";

const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(StructOpt, Debug)]
pub enum MqttCmd {
    /// Publish a MQTT message on a topic.
    Pub {
        /// Topic to publish
        topic: String,
        /// Message to publish
        message: String,
        /// QoS level (0, 1, 2)
        #[structopt(short, long, parse(try_from_str = parse_qos), default_value = "0")]
        qos: QoS,
    },

    /// Subscribe a MQTT topic.
    Sub {
        /// Topic to publish
        topic: String,
        /// QoS level (0, 1, 2)
        #[structopt(short, long, parse(try_from_str = parse_qos), default_value = "0")]
        qos: QoS,
    },
}

#[derive(thiserror::Error, Debug)]
pub enum MqttError {
    #[error("Client error")]
    ConnectError(#[from] mqtt_client::Error),

    #[error("I/O error")]
    IoError(#[from] std::io::Error),

    #[error("Received message is not UTF-8 format")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[error("The input QoS should be 0, 1, or 2")]
    InvalidQoSError,

    #[error("{0}\n\nHint: Is MQTT server running?")]
    ServerError(String),
}

impl BuildCommand for MqttCmd {
    fn build_command(
        self,
        _config: crate::config::TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::config::ConfigError> {
        // Temporary implementation
        // - should return a specific command, not self.
        // - see certificate.rs for an example
        Ok(self.into_boxed())
    }
}

impl Command for MqttCmd {
    fn description(&self) -> String {
        match self {
            MqttCmd::Pub {
                topic,
                message,
                qos,
            } => format!(
                "publish the message \"{}\" on the topic \"{}\" with QoS \"{:?}\".",
                message, topic, qos
            ),
            MqttCmd::Sub { topic, qos } => {
                format!("subscribe the topic \"{}\" with QoS \"{:?}\".", topic, qos)
            }
        }
    }

    fn execute(&self, _verbose: u8, _user_manager: crate::utils::users::UserManager) -> Result<(), anyhow::Error> {
        match self {
            MqttCmd::Pub {
                topic,
                message,
                qos,
            } => publish(topic, message, *qos)?,
            MqttCmd::Sub { topic, qos } => subscribe(topic, *qos)?,
        }
        Ok(())
    }
}

#[tokio::main]
async fn publish(topic: &str, message: &str, qos: QoS) -> Result<(), MqttError> {
    let client_id = format!("{}-{}", PUB_CLIENT_PREFIX, process::id());
    let mut mqtt = Config::new(DEFAULT_HOST, DEFAULT_PORT)
        .connect(client_id.as_str())
        .await?;

    let tpc = Topic::new(topic)?;
    let message = Message::new(&tpc, message).qos(qos);

    let res = try_publish(&mut mqtt, message).await;

    // In case we don't have a connection, disconnect might block until there is a connection,
    // therefore timeout.
    let _ = tokio::time::timeout(DISCONNECT_TIMEOUT, mqtt.disconnect()).await;
    res
}

async fn try_publish(mqtt: &mut Client, msg: Message) -> Result<(), MqttError> {
    let mut errors = mqtt.subscribe_errors();

    // This requires 2 awaits as publish_with_ack returns a future which returns a future.
    let ack = async {
        match mqtt.publish_with_ack(msg).await {
            Ok(fut) => fut.await,
            Err(err) => Err(err),
        }
    };

    select! {
        error = errors.next().fuse() => {
            if let Some(err) = error {
                if let mqtt_client::Error::ConnectionError(..) = *err {
                    return Err(MqttError::ServerError(err.to_string()));
                }
            }
        }

        result = ack.fuse() => {
            result?
        }
    }

    Ok(())
}

#[tokio::main]
async fn subscribe(topic: &str, qos: QoS) -> Result<(), MqttError> {
    let client_id = format!("{}-{}", SUB_CLIENT_PREFIX, process::id());
    let config = Config::new(DEFAULT_HOST, DEFAULT_PORT).clean_session();
    let mqtt = Client::connect(client_id.as_str(), &config).await?;
    let filter = TopicFilter::new(topic)?.qos(qos);

    let mut errors = mqtt.subscribe_errors();
    let mut messages: MessageStream = mqtt.subscribe(filter).await?;

    loop {
        select! {
            error = errors.next().fuse() => {
                if let Some(err) = error {
                    if err.to_string().contains("MQTT connection error: I/O: Connection refused (os error 111)") {
                        return Err(MqttError::ServerError(err.to_string()));
                    }
                }
            }

            _signal = signals::interrupt().fuse() => {
                println!("Received SIGINT.");
                break;
            }

            maybe_message = messages.next().fuse() => {
                match maybe_message {
                    Some(message) =>  handle_message(message).await?,
                    None => break
                 }
            }
        }
    }

    Ok(())
}

async fn async_println(s: &str) -> Result<(), MqttError> {
    let mut stdout = tokio::io::stdout();
    stdout.write_all(s.as_bytes()).await?;
    stdout.write_all(b"\n").await?;
    Ok(())
}

async fn handle_message(message: Message) -> Result<(), MqttError> {
    let s = String::from_utf8(message.payload)?;

    async_println(&s).await?;
    Ok(())
}

pub fn parse_qos(src: &str) -> Result<QoS, MqttError> {
    let int_val: u8 = src.parse().map_err(|_| MqttError::InvalidQoSError)?;
    match int_val {
        0 => Ok(QoS::AtMostOnce),
        1 => Ok(QoS::AtLeastOnce),
        2 => Ok(QoS::ExactlyOnce),
        _ => Err(MqttError::InvalidQoSError),
    }
}

#[cfg(test)]
mod tests {
    use crate::mqtt::parse_qos;
    use mqtt_client::QoS;

    #[test]
    fn test_parse_qos() {
        let input_qos = "0";
        let expected_qos = QoS::AtMostOnce;
        assert_eq!(parse_qos(input_qos).unwrap(), expected_qos);
    }
}
