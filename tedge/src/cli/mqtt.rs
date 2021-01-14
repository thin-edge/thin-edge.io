use crate::cli::MqttCmd;
use futures::future::FutureExt;
use futures::select;
use mqtt_client::{Client, Config, Message, MessageStream, QoS, Topic, TopicFilter};
use tokio::signal::unix::{signal, SignalKind};

const DEFAULT_HOST: &str = "localhost";
const DEFAULT_PORT: u16 = 1883;
const DEFAULT_ID: &str = "tedge-cli";
const DEFAULT_PACKET_ID: u16 = 1;
const DEFAULT_WAIT_FOR_ACK_IN_SEC: u64 = 1;

pub fn parse_qos(src: &str) -> Result<QoS, String> {
    let int_val: u8 = src.parse().map_err(|err| format!("{}", err))?;
    match int_val {
        0 => Ok(QoS::AtMostOnce),
        1 => Ok(QoS::AtLeastOnce),
        2 => Ok(QoS::ExactlyOnce),
        _ => Err(String::from("Should be 0, 1 or 2")),
    }
}

impl crate::cli::MqttCmd {
    pub async fn exec(self) -> Result<(), String> {
        match self {
            MqttCmd::Pub {
                topic,
                message,
                qos,
            } => MqttCmd::publish(&topic, &message, qos).await,
            MqttCmd::Sub { topic, qos } => MqttCmd::subscribe(&topic, qos).await,
        }
    }

    async fn publish(topic: &str, message: &str, qos: QoS) -> Result<(), String> {
        let mqtt = Config::new(DEFAULT_HOST, DEFAULT_PORT)
            .connect(DEFAULT_ID)
            .await
            .unwrap();
        let tpc = Topic::new(topic).unwrap();
        let msg = Message::new(&tpc, message).qos(qos).pkid(DEFAULT_PACKET_ID);
        mqtt.publish_and_wait_for_ack(
            msg,
            std::time::Duration::from_secs(DEFAULT_WAIT_FOR_ACK_IN_SEC),
        )
        .await
        .unwrap();
        mqtt.disconnect().await.unwrap();

        Ok(())
    }

    async fn subscribe(topic: &str, qos: QoS) -> Result<(), String> {
        let config = Config::new(DEFAULT_HOST, DEFAULT_PORT);
        let mqtt = Client::connect(DEFAULT_ID, &config).await.unwrap();
        let filter = TopicFilter::new(topic).unwrap().qos(qos);

        let mut signals = signal(SignalKind::interrupt()).unwrap();
        // signals.recv().await;

        let commands = mqtt.subscribe(filter).await.unwrap();

        select! {
            // _ = listen_topic(commands).fuse() => (),
            message = commands.next() => {
                match message {
                    Some(message) =>  {
                      let s = String::from_utf8(message.payload).unwrap();
                      println!("Received: {}", s);
                    }
                    None => return Ok(())
                 }
            }
        }

        Ok(())
    }
}

async fn listen_topic(mut messages: MessageStream) {
    while let Some(message) = messages.next().await {
        let s = String::from_utf8(message.payload).unwrap();
        println!("Received: {}", s);
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::mqtt::parse_qos;
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
        let assert = cmd
            .args(&["mqtt", "sub", "topic"])
            .timeout(std::time::Duration::from_secs(1))
            .ok();

        match assert {
            Ok(output) => output.assert().failure(),
            Err(e) => e.as_output().unwrap().assert().failure(),
        }

        Ok(())
    }

    #[test]
    fn test_cli_sub_qos() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("tedge")?;
        let assert = cmd
            .args(&["mqtt", "sub", "topic"])
            .args(&["--qos", "1"])
            .timeout(std::time::Duration::from_secs(1))
            .unwrap()
            .assert();

        assert
            .interrupted()
            .failure()
            .stderr(predicate::str::is_empty());
        Ok(())
    }

    #[test]
    fn test_parse_qos() {
        let input_qos = "0";
        let expected_qos = QoS::AtMostOnce;
        assert_eq!(parse_qos(input_qos).unwrap(), expected_qos);
    }
}
