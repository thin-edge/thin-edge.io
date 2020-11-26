// Code share by all the integration tests
// using `use common`.
// See [Submodules in Integration Tests](https://doc.rust-lang.org/book/ch11-03-test-organization.html#submodules-in-integration-tests)

use rumqttc::Event::{Incoming, Outgoing};
use rumqttc::Packet::PubAck;
use rumqttc::Packet::PubComp;
use rumqttc::Packet::PubRel;
use rumqttc::Packet::Publish;
use rumqttc::Packet::SubAck;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use tokio_compat_02::FutureExt;

pub async fn launch_mapper() -> Result<(), mapper::Error> {
    let configuration = mapper::Configuration::default();
    let mut mapper = mapper::EventLoop::new(configuration);
    mapper.run().await
}

pub async fn publish_message(client_id: &str, topic: &str, payload: &[u8]) {
    let mqtt_options = MqttOptions::new(client_id, "localhost", 1883);
    let (mqtt_client, mut eventloop) = AsyncClient::new(mqtt_options, 10);
    mqtt_client
        .publish(topic, QoS::ExactlyOnce, false, payload)
        .compat()
        .await
        .unwrap();

    loop {
        match eventloop.poll().compat().await {
            Ok(Incoming(PubAck(_))) | Ok(Incoming(PubComp(_))) => {
                mqtt_client.disconnect().compat().await.unwrap();
                break;
            }
            Err(err) => {
                panic!("MQTT bus: {}", err);
            }
            _ => (),
        }
    }
}

pub async fn subscribe(client_id: &str, topic: &str) {
    let mut mqtt_options = MqttOptions::new(client_id, "localhost", 1883);
    mqtt_options.set_clean_session(false);
    let (mqtt_client, mut eventloop) = AsyncClient::new(mqtt_options, 10);
    mqtt_client.subscribe(topic, QoS::ExactlyOnce).compat().await.unwrap();

    loop {
        match eventloop.poll().compat().await {
            Ok(Incoming(SubAck(_))) => {
                mqtt_client.disconnect().compat().await.unwrap();
                break;
            }
            Err(err) => {
                panic!("MQTT bus: {}", err);
            }
            _ => (),
        }
    }
}

pub async fn expect_message(client_id: &str, topic: &str) -> Option<String> {
    let mut mqtt_options = MqttOptions::new(client_id, "localhost", 1883);
    mqtt_options.set_clean_session(false);
    let (mqtt_client, mut eventloop) = AsyncClient::new(mqtt_options, 10);
    mqtt_client.subscribe(topic, QoS::ExactlyOnce).compat().await.unwrap();

    let mut received = None;

    println!("{} is listening on {}", client_id, topic);
    loop {
        match eventloop.poll().compat().await {
            Ok(Incoming(Publish(msg))) if msg.topic == topic => {
                let payload = std::str::from_utf8(&msg.payload).unwrap();
                received = Some(payload.to_string());
            }
            Ok(Incoming(PubRel(_)))
            | Ok(Outgoing(rumqttc::Outgoing::PubAck(_)))
            | Ok(Outgoing(rumqttc::Outgoing::PubComp(_))) => {
                mqtt_client.disconnect().compat().await.unwrap();
                break;
            }
            Err(err) => {
                panic!("MQTT bus: {}", err);
            }
            _ => (),
        }
    }

    received
}
