mod mapper;
use open_json::MeasurementRecord;
use rumqttc::{MqttOptions, Client, QoS};
use rumqttc::Event::Incoming;
use rumqttc::Packet::Publish;

fn main() {
    let name = "c8y-mapper";
    let in_topic = "tedge/measurements";
    let out_topic = "c8y/s/us";

    let mqtt_options = MqttOptions::new(name, "localhost", 1883);
    let (mut mqtt_client, mut connection) = Client::new(mqtt_options, 10);

    mqtt_client.subscribe(in_topic, QoS::AtLeastOnce).unwrap();

    eprintln!("Translating: {} -> {}", in_topic, out_topic);
    for notification in connection.iter() {
        match notification {
            Ok(Incoming(Publish(input))) if input.topic == in_topic => {
                let record = match MeasurementRecord::from_bytes(&input.payload) {
                    Ok(rec) => rec,
                    Err(err) => {
                        eprintln!("ERROR reading input: {}", err);
                        break;
                    }
                };
                let messages = match mapper::into_smart_rest(&record) {
                    Ok(messages) => messages,
                    Err(err) => {
                        eprintln!("ERROR translating input: {}", err);
                        break;
                    }
                };
                for msg in messages.into_iter() {
                    if let Some(err) = mqtt_client.publish(out_topic, QoS::AtLeastOnce, false, msg).err() {
                        eprintln!("ERROR publishing output: {}", err);
                    }
                }
            }
            Err(err) => {
                eprintln!("ERROR MQTT: {}", err);
            }
            _ => ()
        }
    }
}
