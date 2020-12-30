use rumqttc::AsyncClient;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::MqttOptions;
use rumqttc::QoS;

use std::error::Error;

const MQTT_ID: &str = "tedge-mapper";
const LOCALHOST: &str = "localhost";
const MQTT_HOST: &str = LOCALHOST;
const MQTT_PORT: u16 = 1883;

pub struct Mapper {
    client: rumqttc::AsyncClient,
    eventloop: rumqttc::EventLoop,
}

impl Mapper {
    pub fn new() -> Mapper {
        log::info!("tedge-mapper starting!");

        let mut mqttoptions = MqttOptions::new(MQTT_ID, MQTT_HOST, MQTT_PORT);
        mqttoptions.set_keep_alive(30);
        mqttoptions.set_connection_timeout(10);

        let (client, eventloop) = AsyncClient::new(mqttoptions, 10);

        Mapper {
            client: client,
            eventloop: eventloop,
        }
    }

    pub async fn connect(&self) -> Result<(), Box<dyn Error>> {
        Ok(self
            .client
            .subscribe("tedge/measurements", QoS::AtMostOnce)
            .await?)
    }

    pub async fn run_forever(&mut self) -> Result<(), Box<dyn Error>> {
        loop {
            match self.eventloop.poll().await {
                Ok(Event::Incoming(Incoming::Publish(p))) => {
                    log::debug!("Topic: {}, Payload: {:?}", p.topic, p.payload)
                }
                Ok(Event::Incoming(i)) => {
                    log::debug!("Incoming = {:?}", i);
                }
                Ok(Event::Outgoing(o)) => println!("Outgoing = {:?}", o),
                Err(e) => {
                    println!("Error = {:?}", e);
                    return Err(Box::new(e));
                }
            }
        }
    }
}
