use mqtt_client::{Config, Message, QoS, Topic};

async fn publish(
    config: &Config,
    qos: QoS,
    payload: impl Into<String>,
) -> Result<(), mqtt_client::Error> {
    let topic = Topic::new("test/uubpb9wyi9asi46l624f")?;
    let mut client = config.connect("publisher").await?;
    let message = Message::new(&topic, payload.into()).qos(qos);

    let _ = client.publish(message).await?;
    client.disconnect().await?;
    Ok(())
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::new("test.mosquitto.org", 1883);
    let payload = "Hello there!";

    env_logger::init();

    publish(&config, QoS::AtMostOnce, payload).await?;
    publish(&config, QoS::AtLeastOnce, payload).await?;
    publish(&config, QoS::ExactlyOnce, payload).await?;

    Ok(())
}
