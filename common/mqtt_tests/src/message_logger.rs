use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};

/// Returns the stream of messages received on a specific topic.
///
/// To ease testing, the errors are returned as messages.
pub async fn messages_published_on(
    mqtt_port: u16,
    topic: &str,
) -> tokio::sync::mpsc::UnboundedReceiver<String> {
    let (sender, recv) = tokio::sync::mpsc::unbounded_channel();

    let mut con = TestCon::new(mqtt_port);

    if let Err(err) = con.subscribe(topic, QoS::AtLeastOnce).await {
        let msg = format!("Error: {:?}", err).to_string();
        let _ = sender.send(msg);
        return recv;
    }

    tokio::spawn(async move {
        con.send_messages(sender).await;
    });

    recv
}

pub struct TestCon {
    client: AsyncClient,
    eventloop: EventLoop,
}

impl TestCon {
    pub fn new(mqtt_port: u16) -> TestCon {
        let id: String = std::iter::repeat_with(fastrand::alphanumeric)
            .take(10)
            .collect();
        let mut options = MqttOptions::new(id, "localhost", mqtt_port);
        options.set_clean_session(true);

        let (client, eventloop) = AsyncClient::new(options, 10);
        TestCon { client, eventloop }
    }

    pub async fn subscribe(&mut self, topic: &str, qos: QoS) -> Result<(), anyhow::Error> {
        self.client.subscribe(topic, qos).await?;

        loop {
            match self.eventloop.poll().await {
                Ok(Event::Incoming(Packet::SubAck(_))) => {
                    return Ok(());
                }
                Err(err) => {
                    return Err(err)?;
                }
                _ => {}
            }
        }
    }

    pub async fn send_messages(&mut self, sender: tokio::sync::mpsc::UnboundedSender<String>) {
        loop {
            match self.eventloop.poll().await {
                Ok(Event::Incoming(Packet::Publish(response))) => {
                    let msg = std::str::from_utf8(&response.payload)
                        .unwrap_or("Error: non-utf8-payload")
                        .to_string();
                    if let Err(_) = sender.send(msg) {
                        break;
                    }
                }
                Err(err) => {
                    let msg = format!("Error: {:?}", err).to_string();
                    let _ = sender.send(msg);
                    break;
                }
                _ => {}
            }
        }
        let _ = self.client.disconnect().await;
    }
}
