use rumqttc::QoS::AtLeastOnce;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, Packet};

pub async fn messages_published_on(
    mqtt_port: u16,
    topic: &str,
) -> Result<tokio::sync::mpsc::UnboundedReceiver<String>, anyhow::Error> {
    let mut options = MqttOptions::new("logger", "localhost", mqtt_port);
    options.set_clean_session(true);

    let (client, mut eventloop) = AsyncClient::new(options, 10);
    client.subscribe(topic, AtLeastOnce).await?;

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the messages
                break;
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                client.disconnect().await?;
                return Err(anyhow::anyhow!("Disconnected"));
            }
            Err(err) => {
                client.disconnect().await?;
                return Err(err.into());
            }
            _ => {}
        }
    }

    let (send, recv) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::Publish(response))) => {
                    let msg = std::str::from_utf8(&response.payload)
                        .unwrap_or("Error: non-utf8-payload")
                        .to_string();
                    if let Err(_) = send.send(msg) {
                        break;
                    }
                }
                Ok(Event::Incoming(Incoming::Disconnect)) => {
                    let msg = "Error: disconnected".to_string();
                    let _ = send.send(msg);
                    break;
                }
                Err(err) => {
                    let msg = format!("Error: {:?}", err).to_string();
                    let _ = send.send(msg);
                    break;
                }
                _ => {}
            }
        }
    });

    Ok(recv)
}
