use mqtt_client::{Config, Message, QoS, Topic};

async fn publish(
    config: &Config,
    qos: QoS,
    payload: impl Into<String>,
) -> Result<(), mqtt_client::Error> {
    let topic = Topic::new("test/uubpb9wyi9asi46l624f")?;
    let client = config.connect("publisher").await?;
    let message = Message::new(&topic, payload.into()).qos(qos);

    let _ = client.publish(message).await?;
    client.disconnect().await?;
    Ok(())
}

async fn publish_inflight(
    config: &Config,
    payload: impl Into<String>,
) -> Result<(), mqtt_client::Error> {
    let topic = Topic::new("test/uubpb9wyi9asi46l624f")?;
    let client = config.connect("publisher").await?;

    let payload: String = payload.into();

    let message1 = Message::new(&topic, payload.clone()).qos(QoS::ExactlyOnce);
    let message2 = Message::new(&topic, payload).qos(QoS::ExactlyOnce);

    let ack1 = client.publish_with_ack(message1);
    let ack2 = client.publish_with_ack(message2);

    // Both `message1` and `message2` are issued concurrently.
    // Note that the order in which they are sent is undefined.
    let ((), ()) = tokio::try_join!(ack1, ack2)?;

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

    publish_inflight(&config, payload).await?;

    Ok(())
}
