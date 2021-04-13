use crate::cli::mqtt::{publish::MqttPublishCommand, subscribe::MqttSubscribeCommand, MqttError};
use crate::command::{BuildCommand, BuildContext, Command};
use mqtt_client::{QoS, Topic};
use std::time::Duration;
use structopt::StructOpt;

const DEFAULT_HOST: &str = "localhost";
const DEFAULT_PORT: u16 = 1883;
const PUB_CLIENT_PREFIX: &str = "tedge-pub";
const SUB_CLIENT_PREFIX: &str = "tedge-sub";
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(StructOpt, Debug)]
pub enum TEdgeMqttCli {
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
        /// Avoid printing the message topics on the console
        #[structopt(long = "no-topic")]
        hide_topic: bool,
    },
}

impl BuildCommand for TEdgeMqttCli {
    fn build_command(
        self,
        _context: BuildContext,
    ) -> Result<Box<dyn Command>, crate::config::ConfigError> {
        let cmd = {
            match self {
                TEdgeMqttCli::Pub {
                    topic,
                    message,
                    qos,
                } => MqttPublishCommand {
                    topic: Topic::new(topic.as_str())?,
                    message,
                    qos,
                    mqtt_config: mqtt_client::Config::new(DEFAULT_HOST, DEFAULT_PORT),
                    client_id: format!("{}-{}", PUB_CLIENT_PREFIX, std::process::id()),
                    disconnect_timeout: DISCONNECT_TIMEOUT,
                }
                .into_boxed(),
                TEdgeMqttCli::Sub {
                    topic,
                    qos,
                    hide_topic,
                } => MqttSubscribeCommand {
                    topic,
                    qos,
                    hide_topic,
                    mqtt_config: mqtt_client::Config::new(DEFAULT_HOST, DEFAULT_PORT),
                    client_id: format!("{}-{}", SUB_CLIENT_PREFIX, std::process::id()),
                }
                .into_boxed(),
            }
        };

        Ok(cmd)
    }
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

#[test]
fn test_parse_qos() {
    let input_qos = "0";
    let expected_qos = QoS::AtMostOnce;
    assert_eq!(parse_qos(input_qos).unwrap(), expected_qos);
}
