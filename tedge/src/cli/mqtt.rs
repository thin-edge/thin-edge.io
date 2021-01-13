use crate::cli::MqttCmd;
use futures::future::FutureExt;
use futures::select;
use mqtt_client::{Client, Config, Topic, Message, MessageStream, QoS, TopicFilter};

const DEFAULT_HOST: &str = "test.mosquitto.org";
const DEFAULT_PORT: u16 = 1883;
const DEFAULT_ID: &str = "rumqtt-sync";

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
        let c8y_msg = Topic::new(topic).unwrap();
        let msg = Message::new(&c8y_msg, message).qos(qos).pkid(4);
        mqtt.publish_and_wait_for_ack(msg, std::time::Duration::from_secs(2))
            .await
            .unwrap();
        mqtt.disconnect().await.unwrap();

        Ok(())
    }

    async fn subscribe(topic: &str, qos: QoS) -> Result<(), String> {
        let config = Config::new(DEFAULT_HOST, DEFAULT_PORT);
        let mqtt = Client::connect(DEFAULT_ID, &config).await.unwrap();
        let filter = TopicFilter::new(topic).unwrap().qos(qos);

        let commands = mqtt.subscribe(filter).await.unwrap();

        select! {
            _ = listen_topic(commands).fuse() => (),
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
