use std::time::Duration;
use rand::prelude::*;
use tokio::time::sleep;
use mqtt_client::{Client,Topic,Message};

const C8Y_TPL_RESTART: &str = "511";
const C8Y_TPL_TEMPERATURE: &str = "211";

#[tokio::main]
pub async fn main() -> Result<(), mqtt_client::Error> {
    let mqtt = Client::connect("temperature").await?;
    let c8y_msg = Topic::new("c8y/s/us");
    let c8y_cmd = Topic::new("c8y/s/ds");
    let c8y_err = Topic::new("c8y/s/e");

    let mut messages = mqtt.subscribe(&c8y_cmd).await?;

    tokio::spawn(async move {
        while let Some(message) = messages.next().await {
            if message.topic == c8y_cmd {
                println!("C8Y command: {:?}", message.payload);
            }
            else if message.topic == c8y_err {
                println!("C8Y error: {:?}", message.payload);
            }
        }
    });

    let mut rng = thread_rng();
    let mut temperature : i32 = rng.gen_range(-10, 20);
    loop {
        let delta = rng.gen_range(-1, 2);
        temperature = temperature + delta;

        let payload = format!("{},{}",C8Y_TPL_TEMPERATURE,temperature);
        mqtt.publish(Message::new(&c8y_msg, payload)).await.unwrap();

        sleep(Duration::from_millis(1000)).await;
    }
}
