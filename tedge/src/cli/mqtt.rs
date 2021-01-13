use crate::cli::MqttCmd;
use futures::future::FutureExt;
use futures::select;
use mqtt_client::{Client, Config, Message, MessageStream, QoS, Topic, TopicFilter};

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
        let mqtt = Config::new(DEFAULT_HOST.into(), DEFAULT_PORT)
            .connect(DEFAULT_ID)
            .await
            .unwrap();
        let c8y_msg = Topic::new(topic).unwrap();
        mqtt.publish(Message::new_with_qos(&c8y_msg, qos, message))
            .await
            .unwrap();
        mqtt.disconnect().await.unwrap();
        Ok(())
    }

    async fn subscribe(topic: &str, qos: QoS) -> Result<(), String> {
        let config = Config::new(DEFAULT_HOST.into(), DEFAULT_PORT);
        let mqtt = Client::connect(DEFAULT_ID, &config).await.unwrap();
        let filter = TopicFilter::new_with_qos(topic, qos).unwrap();

        let mut commands = mqtt.subscribe(filter).await.unwrap();

        select! {
            _ = listen_topic(commands).fuse() => (),
        }

        Ok(())
    }
}

async fn listen_topic(mut messages: MessageStream) {
    while let Some(message) = messages.next().await {
        println!("Received!: {:?}", message.payload);
    }
}

// fn publish(topic: &String, message: &String, qos: &rumqttc::QoS) -> Result<(), String> {
//     let mut mqttoptions = MqttOptions::new(DEFAULT_ID, DEFAULT_HOST, DEFAULT_PORT);
//     mqttoptions.set_keep_alive(30);
//
//     let (mut client, mut connection) = Client::new(mqttoptions, 10);
//
//     let topic_clone = topic.clone();
//     let qos_clone = qos.clone();
//     let message_clone = message.clone();
//
//     thread::spawn(move || {
//         client
//             .publish(topic_clone, qos_clone, false, message_clone)
//             .map_err(|e| e.to_string());
//         thread::sleep(Duration::from_millis(100));
//     });
//
//     for (i, notification) in connection.iter().enumerate() {
//         println!("Notification = {:?}", notification);
//     }
//
//     Ok(())
// }
//
// fn subscribe(topic: &String, qos: &rumqttc::QoS) -> Result<(), String> {
//     let mut mqttoptions = MqttOptions::new(DEFAULT_ID, DEFAULT_HOST, DEFAULT_PORT);
//     mqttoptions.set_keep_alive(30);
//
//     let (mut client, mut connection) = Client::new(mqttoptions, 10);
//
//     let topic_clone = topic.clone();
//     let qos_clone = qos.clone();
//
//     client
//         .subscribe(topic_clone, qos_clone)
//         .map_err(|e| e.to_string());
//
//     for (i, notification) in connection.iter().enumerate() {
//         println!("Notification = {:?}", notification);
//     }
//
//     Ok(())
// }

//  #[tokio::main]
// async fn publish(topic: &String, message: &String, qos: &rumqttc::QoS) -> Result<(), String> {
//     let mut mqttoptions = MqttOptions::new(DEFAULT_ID, DEFAULT_HOST, DEFAULT_PORT);
//     mqttoptions.set_keep_alive(5);
//
//     let (mut client, mut eventloop) = AsyncClient::new(mqttoptions, 10);
//
//     task::spawn(async move {
//         client
//             .publish("testtest", rumqttc::QoS::ExactlyOnce, false, "bbbbbbbb")
//             .await
//             .map_err(|e| e.to_string());
//         time::sleep(Duration::from_millis(1000)).await;
//     });
//
//     loop {
//         let notification = eventloop.poll().await.map_err(|e| e.to_string());
//         println!("Received = {:?}", notification);
//         tokio::time::sleep(Duration::from_secs(1)).await;
//     }
//
//     Ok(())
// }

// #[tokio::main]
// async fn subscribe(topic: &String, qos: &rumqttc::QoS) -> Result<(), String> {
//     let mut mqttoptions = MqttOptions::new("rumqtt-async", "test.mosquitto.org", 1883);
//     mqttoptions.set_keep_alive(5);
//
//     let (mut client, mut eventloop) = AsyncClient::new(mqttoptions, 10);
//     client.subscribe("testest", rumqttc::QoS::AtMostOnce).await.unwrap();
//
//     loop {
//         let notification = eventloop.poll().await.unwrap();
//         println!("Received = {:?}", notification);
//         tokio::time::sleep(Duration::from_secs(1)).await;
//     }
//
//     Ok(())
// }
// }
