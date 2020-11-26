// Code share by all the integration tests
// using `use common`.
// See [Submodules in Integration Tests](https://doc.rust-lang.org/book/ch11-03-test-organization.html#submodules-in-integration-tests)

use rumqttc::Event::Incoming;
use rumqttc::Packet::PubAck;
use rumqttc::Packet::PubComp;
use rumqttc::Packet::Publish;
use rumqttc::Packet::SubAck;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use tokio_compat_02::FutureExt;

pub async fn launch_mapper() -> Result<(), mapper::Error> {
    let configuration = mapper::Configuration::default();
    let mut mapper = mapper::EventLoop::new(configuration);
    mapper.run().await
}

pub struct MqttClient {
    mqtt_client: rumqttc::AsyncClient,
    eventloop: rumqttc::EventLoop,
}

impl MqttClient {

    pub fn new(client_id: &str) -> MqttClient {
        let mqtt_options = MqttOptions::new(client_id, "localhost", 1883);
        let (mqtt_client, eventloop) = AsyncClient::new(mqtt_options, 10);

        MqttClient {
            mqtt_client,
            eventloop,
        }
    }

    pub async fn publish(&mut self, topic: &str, payload: &[u8]) {
        let mqtt_client = &self.mqtt_client;
        let eventloop = &mut self.eventloop;

        mqtt_client
            .publish(topic, QoS::ExactlyOnce, false, payload)
            .compat()
            .await
            .unwrap();

        loop {
            match eventloop.poll().compat().await {
                Ok(Incoming(PubAck(_))) | Ok(Incoming(PubComp(_))) => {
                    break;
                }
                Err(err) => {
                    panic!("MQTT bus: {}", err);
                }
                _ => (),
            }
        }
    }

    pub async fn subscribe(&mut self, topic: &str) {
        let mqtt_client = &self.mqtt_client;
        let eventloop = &mut self.eventloop;

        mqtt_client.subscribe(topic, QoS::ExactlyOnce).compat().await.unwrap();

        loop {
            match eventloop.poll().compat().await {
                Ok(Incoming(SubAck(_))) => {
                    break;
                }
                Err(err) => {
                    panic!("MQTT bus: {}", err);
                }
                _ => (),
            }
        }
    }

    pub async fn expect_message(&mut self, topic: &str) -> Option<String> {
        let eventloop = &mut self.eventloop;

        loop {
            match eventloop.poll().compat().await {
                Ok(Incoming(Publish(msg))) if msg.topic == topic => {
                    let payload = std::str::from_utf8(&msg.payload).unwrap();
                    return Some(payload.to_string());
                }
                Err(err) => {
                    panic!("MQTT bus: {}", err);
                }
                _ => (),
            }
        }
    }
}
