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

async fn pipelined_publish(
    config: &Config,
    payload: impl Into<String>,
) -> Result<(), mqtt_client::Error> {
    let topic = Topic::new("test/uubpb9wyi9asi46l624f")?;
    let client = config.connect("publisher").await?;

    let payload: String = payload.into();

    let message1 = Message::new(&topic, payload.clone()).qos(QoS::ExactlyOnce);
    let message2 = Message::new(&topic, payload).qos(QoS::ExactlyOnce);

    let fut1 = client.publish(message1).await?;
    let fut2 = client.publish(message2).await?;

    let () = fut2.await?;
    let () = fut1.await?;

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

    pipelined_publish(&config, payload).await?;

    Ok(())
}
