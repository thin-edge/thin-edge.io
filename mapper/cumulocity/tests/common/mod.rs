// Code share by all the integration tests
// using `use common`.
// See [Submodules in Integration Tests](https://doc.rust-lang.org/book/ch11-03-test-organization.html#submodules-in-integration-tests)

use rumqttc::{MqttOptions, Client, QoS};
use rumqttc::Event::Incoming;
use rumqttc::Packet::Publish;
use rumqttc::Packet::PubAck;
use rumqttc::Packet::PubComp;
use rumqttc::Packet::SubAck;
use mapper::Error;

pub fn launch_mapper() -> Result<(), Error> {
    mapper::run("c8y-mapper", "tedge/measurements", "c8y/s/us", exit)
}

fn exit(err: mapper::Error) ->  Result<(),mapper::Error> {
    Err(err)
}

pub fn publish_message(client_id: &str, topic: &str, payload: &[u8]) {
    let mqtt_options = MqttOptions::new(client_id, "localhost", 1883);
    let (mut mqtt_client, mut connection) = Client::new(mqtt_options, 10);
    mqtt_client.publish(topic, QoS::ExactlyOnce, false, payload).unwrap();

    for notification in connection.iter() {
        match notification {
            Ok(Incoming(PubAck(_))) | Ok(Incoming(PubComp(_))) => {
                mqtt_client.disconnect().unwrap();
                break;
            },
            Err(err) => {
                panic!("MQTT bus: {}", err);
            }
            _ => ()
        }
    }
}

pub fn subscribe(client_id: &str, topic: &str) {
    let mut mqtt_options = MqttOptions::new(client_id, "localhost", 1883);
    mqtt_options.set_clean_session(false);
    let (mut mqtt_client, mut connection) = Client::new(mqtt_options, 10);
    mqtt_client.subscribe(topic, QoS::ExactlyOnce).unwrap();

    for notification in connection.iter() {
        match notification {
            Ok(Incoming(SubAck(_))) => {
                mqtt_client.disconnect().unwrap();
                break;
            }
            Err(err) => {
                panic!("MQTT bus: {}", err);
            }
            _ => ()
        }
    }
}

pub fn expect_message(client_id: &str, topic: &str) -> Option<String> {
    let mut mqtt_options = MqttOptions::new(client_id, "localhost", 1883);
    mqtt_options.set_clean_session(false);
    let (mut mqtt_client, mut connection) = Client::new(mqtt_options, 10);
    mqtt_client.subscribe(topic, QoS::ExactlyOnce).unwrap();

    println!("{} is listening on {}", client_id, topic);
    for notification in connection.iter() {
        match notification {
            Ok(Incoming(Publish(msg))) if msg.topic == topic => {
                let payload = std::str::from_utf8(&msg.payload).unwrap();
                mqtt_client.disconnect().unwrap();
                return Some(payload.to_string());
            }
            Err(err) => {
                panic!("MQTT bus: {}", err);
            }
            e => println!("Got {:?}", e)
        }
    }
    None
}

