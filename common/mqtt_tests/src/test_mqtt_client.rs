use crate::with_timeout::WithTimeout;
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

/// Returns the stream of messages received on a specific topic.
///
/// To ease testing, the errors are returned as messages.
pub async fn messages_published_on(mqtt_port: u16, topic: &str) -> UnboundedReceiver<String> {
    let (sender, recv) = tokio::sync::mpsc::unbounded_channel();

    let mut con = TestCon::new(mqtt_port);

    if let Err(err) = con.subscribe(topic, QoS::AtLeastOnce).await {
        let msg = format!("Error: {:?}", err).to_string();
        let _ = sender.send(msg);
        return recv;
    }

    tokio::spawn(async move {
        con.forward_received_messages(sender).await;
    });

    recv
}

/// Check that a list of messages has been received in the given order
pub async fn assert_received<T>(
    messages: &mut UnboundedReceiver<String>,
    timeout: Duration,
    expected: T,
) where
    T: IntoIterator,
    T::Item: ToString,
{
    for expected_msg in expected.into_iter() {
        let actual_msg = messages.recv().with_timeout(timeout).await;
        assert_eq!(actual_msg, Ok(Some(expected_msg.to_string())));
    }
}

/// Publish a message
///
/// Return only when the message has been acknowledged.
pub async fn publish(mqtt_port: u16, topic: &str, payload: &str) -> Result<(), anyhow::Error> {
    let mut con = TestCon::new(mqtt_port);

    con.publish(topic, QoS::AtLeastOnce, payload).await
}

/// Publish the `pub_message` on the `pub_topic` only when ready to receive a message on `sub_topic`.
///
/// 1. Subscribe to the `sub_topic`,
/// 2. Wait for the acknowledgment of the subscription
/// 3  Publish the `pub_message` on the `pub_topic`,
/// 4. Return the first received message
/// 5. or give up after `timeout_sec` secondes.
pub async fn wait_for_response_on_publish(
    mqtt_port: u16,
    pub_topic: &str,
    pub_message: &str,
    sub_topic: &str,
    timeout: Duration,
) -> Option<String> {
    let mut con = TestCon::new(mqtt_port);

    con.subscribe(sub_topic, QoS::AtLeastOnce).await.ok()?;
    con.publish(pub_topic, QoS::AtLeastOnce, pub_message)
        .await
        .ok()?;
    match tokio::time::timeout(timeout, con.next_message()).await {
        // One collapse both timeout and error to None
        Err(_) | Ok(Err(_)) => None,
        Ok(Ok(x)) => Some(x),
    }
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

    pub async fn publish(
        &mut self,
        topic: &str,
        qos: QoS,
        payload: &str,
    ) -> Result<(), anyhow::Error> {
        self.client.publish(topic, qos, false, payload).await?;

        loop {
            match self.eventloop.poll().await {
                Ok(Event::Incoming(Packet::PubAck(_))) => {
                    return Ok(());
                }
                Err(err) => {
                    return Err(err)?;
                }
                _ => {}
            }
        }
    }

    pub async fn forward_received_messages(&mut self, sender: UnboundedSender<String>) {
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

    pub async fn next_message(&mut self) -> Result<String, anyhow::Error> {
        loop {
            match self.eventloop.poll().await {
                Ok(Event::Incoming(Packet::Publish(packet))) => {
                    let msg = std::str::from_utf8(&packet.payload)
                        .unwrap_or("Error: non-utf8-payload")
                        .to_string();
                    return Ok(msg);
                }
                Err(err) => {
                    return Err(err)?;
                }
                _ => {}
            }
        }
    }
}
