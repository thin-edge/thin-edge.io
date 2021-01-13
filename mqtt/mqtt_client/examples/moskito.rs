use log::error;
use mqtt_client::{AckFilter, Client, Config, Message, QoS, Topic};

#[tokio::main]
pub async fn main() -> Result<(), mqtt_client::Error> {
    env_logger::init();

    let mqtt = Client::connect("test", &Config::new("test.mosquitto.org", 1883)).await?;
    let mut errors = mqtt.subscribe_errors();
    tokio::spawn(async move {
        while let Some(error) = errors.next().await {
            error!("{}", error);
        }
    });

    let topic = Topic::new("c8y/s/us").unwrap();
    let msg = Message::new(&topic, "211,23").qos(QoS::AtLeastOnce).pkid(4);

    {
        let msg = msg.clone();
        let mut acks = mqtt.subscribe_acks();
        let ack = acks.filter(AckFilter::Id(msg.pkid));

        mqtt.publish(msg).await.unwrap();

        let puback = ack.await.unwrap();
        println!("{:?}", puback);
    }

    // Or simpler:

    {
        let pub_ack = mqtt
            .publish_and_wait_for_ack(msg, std::time::Duration::from_secs(1))
            .await
            .unwrap();
        println!("{:?}", pub_ack);
    }

    mqtt.disconnect().await.unwrap();

    Ok(())
}
