use super::command::Command;
use futures::future::FutureExt;
use futures::select;
use mqtt_client::{Client, Config, Message, MessageStream, QoS, Topic, TopicFilter};
use structopt::StructOpt;
use tokio::signal::unix::{signal, SignalKind};

const DEFAULT_HOST: &str = "localhost";
const DEFAULT_PORT: u16 = 1883;
const DEFAULT_ID: &str = "tedge-cli";
const DEFAULT_PACKET_ID: u16 = 1;
const DEFAULT_WAIT_FOR_ACK_IN_SEC: u64 = 1;

#[derive(StructOpt, Debug)]
pub enum MqttCmd {
    /// Publish a MQTT message on a topic.
    Pub {
        /// Topic to publish
        topic: String,
        /// Message to publish
        message: String,
        ///  QoS level (0, 1, 2)
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
    #[error(r#"Client error"#)]
    ConnectError(#[from] mqtt_client::Error),

    #[error("I/O error")]
    IoError(#[from] std::io::Error),
}

impl Command for MqttCmd {
    fn to_string(&self) -> String {
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

    fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            match self {
                MqttCmd::Pub {
                    topic,
                    message,
                    qos,
                } => publish(topic, message, qos).await?,
                MqttCmd::Sub { topic, qos } => subscribe(&topic, qos).await?,
            }
            Ok(())
        })
    }
}

async fn publish(topic: &str, message: &str, qos: &QoS) -> Result<(), MqttError> {
    let mqtt = Config::new(DEFAULT_HOST, DEFAULT_PORT)
        .connect(DEFAULT_ID)
        .await?;
    let tpc = Topic::new(topic)?;
    let msg = Message::new(&tpc, message)
        .qos(*qos)
        .pkid(DEFAULT_PACKET_ID);
    mqtt.publish_and_wait_for_ack(
        msg,
        std::time::Duration::from_secs(DEFAULT_WAIT_FOR_ACK_IN_SEC),
    )
    .await?;
    mqtt.disconnect().await?;

    Ok(())
}

async fn subscribe(topic: &str, qos: &QoS) -> Result<(), MqttError> {
    let config = Config::new(DEFAULT_HOST, DEFAULT_PORT);
    let mqtt = Client::connect(DEFAULT_ID, &config).await?;
    let filter = TopicFilter::new(topic).unwrap().qos(*qos);

    let mut signals = signal(SignalKind::interrupt())?;
    let mut messages: MessageStream = mqtt.subscribe(filter).await?;

    loop {
        select! {
            _signal = signals.recv().fuse() => {
                println!("Received SIGINT.");
                break;
            }

            maybe_message = messages.next().fuse() => {
                match maybe_message {
                    Some(message) =>  handle_message(message),
                    None => break
                 }
            }
        }
    }

    Ok(())
}

fn handle_message(message: Message) {
    let s = String::from_utf8(message.payload).unwrap();
    println!("Received: {}", s);
}

pub fn parse_qos(src: &str) -> Result<QoS, String> {
    let int_val: u8 = src.parse().map_err(|err| format!("{}", err))?;
    match int_val {
        0 => Ok(QoS::AtMostOnce),
        1 => Ok(QoS::AtLeastOnce),
        2 => Ok(QoS::ExactlyOnce),
        _ => Err(String::from("QoS must be 0, 1 or 2")),
    }
}

#[cfg(test)]
#[cfg(not(target_arch = "arm"))] // Need CIT-160 resolution
mod tests {
    use crate::mqtt::parse_qos;
    use assert_cmd::prelude::*;
    use assert_cmd::Command;
    use mqtt_client::QoS;
    use predicates::prelude::*;

    #[test]
    fn test_cli_pub_basic() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("tedge")?;
        let assert = cmd
            .args(&["mqtt", "pub", "topic", "message"])
            .unwrap()
            .assert();

        assert.success().code(predicate::eq(0));
        Ok(())
    }

    #[test]
    fn test_cli_pub_qos() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("tedge")?;
        let assert = cmd
            .args(&["mqtt", "pub", "topic", "message"])
            .args(&["--qos", "1"])
            .unwrap()
            .assert();

        assert.success().code(predicate::eq(0));
        Ok(())
    }

    #[test]
    fn test_cli_sub_basic() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("tedge")?;
        let err = cmd
            .args(&["mqtt", "sub", "topic"])
            .timeout(std::time::Duration::from_secs(1))
            .unwrap_err();

        let output = err.as_output().unwrap();
        assert_eq!(None, output.status.code());

        Ok(())
    }

    #[test]
    fn test_cli_sub_qos() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("tedge")?;
        let err = cmd
            .args(&["mqtt", "sub", "topic"])
            .args(&["--qos", "1"])
            .timeout(std::time::Duration::from_secs(1))
            .unwrap_err();

        let output = err.as_output().unwrap();
        assert_eq!(None, output.status.code());

        Ok(())
    }

    #[test]
    fn test_parse_qos() {
        let input_qos = "0";
        let expected_qos = QoS::AtMostOnce;
        assert_eq!(parse_qos(input_qos).unwrap(), expected_qos);
    }
}
