use crate::cli::mqtt::publish::MqttPublishCommand;
use crate::cli::mqtt::subscribe::MqttSubscribeCommand;
use crate::cli::mqtt::subscribe::SimpleTopicFilter;
use crate::command::BuildCommand;
use crate::command::Command;
use clap_complete::ArgValueCandidates;
use clap_complete::CompletionCandidate;
use mqtt_channel::Topic;
use rumqttc::QoS;
use tedge_config::models::SecondsOrHumanTime;
use tedge_config::TEdgeConfig;

const PUB_CLIENT_PREFIX: &str = "tedge-pub";
const SUB_CLIENT_PREFIX: &str = "tedge-sub";

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeMqttCli {
    /// Publish a MQTT message on a topic.
    Pub {
        /// Topic to publish
        #[arg(value_parser = Topic::new)]
        topic: Topic,
        /// Message to publish
        message: String,
        /// QoS level (0, 1, 2)
        #[clap(short, long, default_value = "1")]
        #[arg(value_parser = parse_qos)]
        #[arg(add = ArgValueCandidates::new(qos_completions))]
        qos: QoS,
        /// Retain flag
        #[clap(short, long = "retain")]
        retain: bool,
        /// Decode the payload before publishing
        #[clap(long)]
        base64: bool,
        /// Repeat the message
        #[clap(long)]
        repeat: Option<u32>,
        /// Pause between repeated messages (e.g., 60s, 1h)
        #[clap(long, default_value = "1s")]
        sleep: SecondsOrHumanTime,
        /// Give up connecting to the broker after this duration (e.g., 10s, 500ms).
        /// Use 0 / 0s / 0ms for no timeout (retry forever).
        #[clap(long, default_value = "10s")]
        connection_timeout: SecondsOrHumanTime,
    },

    /// Subscribe a MQTT topic.
    Sub {
        /// Topic to subscribe to
        #[arg(value_parser = SimpleTopicFilter::new)]
        topic: SimpleTopicFilter,
        /// QoS level (0, 1, 2)
        #[clap(short, long, default_value = "0")]
        #[arg(value_parser = parse_qos)]
        #[arg(add = ArgValueCandidates::new(qos_completions))]
        qos: QoS,
        /// Avoid printing the message topics on the console
        #[clap(long = "no-topic")]
        hide_topic: bool,
        /// Encode the received payloads
        #[clap(long)]
        base64: bool,
        /// Disconnect and exit after the specified timeout (e.g., 60s, 1h)
        #[clap(long, short = 'W')]
        duration: Option<SecondsOrHumanTime>,
        /// Disconnect and exit after receiving the specified number of messages
        #[clap(long, short = 'C')]
        count: Option<u32>,
        /// Only show retained messages and disconnect and exit after receiving
        /// the first non-retained message
        #[clap(long)]
        retained_only: bool,
        /// Give up connecting to the broker after this duration (e.g., 10s, 500ms).
        /// Use 0 / 0s / 0ms for no timeout (retry forever).
        #[clap(long, default_value = "10s")]
        connection_timeout: SecondsOrHumanTime,
    },
}

#[async_trait::async_trait]
impl BuildCommand for TEdgeMqttCli {
    async fn build_command(
        self,
        config: &TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::ConfigError> {
        let auth_config = config.mqtt_client_auth_config();

        let cmd = {
            match self {
                TEdgeMqttCli::Pub {
                    topic,
                    message,
                    qos,
                    retain,
                    base64,
                    repeat,
                    sleep,
                    connection_timeout,
                } => MqttPublishCommand {
                    host: config.mqtt.client.host.clone(),
                    port: config.mqtt.client.port.into(),
                    topic,
                    message,
                    qos,
                    client_id: format!("{}-{}", PUB_CLIENT_PREFIX, std::process::id()),
                    retain,
                    base64,
                    auth_config,
                    count: repeat.unwrap_or(1),
                    sleep: sleep.duration(),
                    connection_timeout: optional_connection_timeout(connection_timeout),
                }
                .into_boxed(),
                TEdgeMqttCli::Sub {
                    topic,
                    qos,
                    hide_topic,
                    base64,
                    duration,
                    count,
                    retained_only,
                    connection_timeout,
                } => MqttSubscribeCommand {
                    host: config.mqtt.client.host.clone(),
                    port: config.mqtt.client.port.into(),
                    topic,
                    qos,
                    hide_topic,
                    base64,
                    client_id: format!("{}-{}", SUB_CLIENT_PREFIX, std::process::id()),
                    auth_config,
                    duration: duration.map(|v| v.duration()),
                    count,
                    retained_only,
                    connection_timeout: optional_connection_timeout(connection_timeout),
                }
                .into_boxed(),
            }
        };

        Ok(cmd)
    }
}

/// Map a CLI duration to an optional connection timeout.
///
/// Zero (including `0`, `0s`, `0ms`) means no timeout / infinite retries.
fn optional_connection_timeout(timeout: SecondsOrHumanTime) -> Option<std::time::Duration> {
    let duration = timeout.duration();
    if duration.is_zero() {
        None
    } else {
        Some(duration)
    }
}

fn parse_qos(src: &str) -> Result<QoS, MqttError> {
    let int_val: u8 = src.parse().map_err(|_| MqttError::InvalidQoS)?;
    match int_val {
        0 => Ok(QoS::AtMostOnce),
        1 => Ok(QoS::AtLeastOnce),
        2 => Ok(QoS::ExactlyOnce),
        _ => Err(MqttError::InvalidQoS),
    }
}

#[derive(thiserror::Error, Debug)]
pub enum MqttError {
    #[error("The input QoS should be 0, 1, or 2")]
    InvalidQoS,
}

fn qos_completions() -> Vec<CompletionCandidate> {
    vec![
        CompletionCandidate::new("0").help(Some("At most once".into())),
        CompletionCandidate::new("1").help(Some("At least once".into())),
        CompletionCandidate::new("2").help(Some("Exactly once".into())),
    ]
}

#[cfg(test)]
mod tests {
    use super::optional_connection_timeout;
    use super::parse_qos;
    use rumqttc::QoS;
    use std::str::FromStr;
    use std::time::Duration;
    use tedge_config::models::SecondsOrHumanTime;

    #[test]
    fn test_parse_qos_at_most_once() {
        let input_qos = "0";
        let expected_qos = QoS::AtMostOnce;
        assert_eq!(parse_qos(input_qos).unwrap(), expected_qos);
    }

    #[test]
    fn test_parse_qos_at_least_once() {
        let input_qos = "1";
        let expected_qos = QoS::AtLeastOnce;
        assert_eq!(parse_qos(input_qos).unwrap(), expected_qos);
    }

    #[test]
    fn test_parse_qos_exactly_once() {
        let input_qos = "2";
        let expected_qos = QoS::ExactlyOnce;
        assert_eq!(parse_qos(input_qos).unwrap(), expected_qos);
    }

    #[test]
    fn zero_connection_timeout_means_infinite_retries() {
        for input in ["0", "0s", "0ms"] {
            let timeout = SecondsOrHumanTime::from_str(input).unwrap();
            assert_eq!(
                optional_connection_timeout(timeout),
                None,
                "input {input:?} should disable the connection timeout"
            );
        }
    }

    #[test]
    fn non_zero_connection_timeout_is_preserved() {
        let timeout = SecondsOrHumanTime::from_str("10s").unwrap();
        assert_eq!(
            optional_connection_timeout(timeout),
            Some(Duration::from_secs(10))
        );
    }
}
