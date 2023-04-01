use crate::with_timeout::WithTimeout;
use futures::channel::mpsc::UnboundedReceiver;
use futures::channel::mpsc::UnboundedSender;
use futures::SinkExt;
use futures::StreamExt;
use rumqttc::AsyncClient;
use rumqttc::Event;
use rumqttc::EventLoop;
use rumqttc::MqttOptions;
use rumqttc::Packet;
use rumqttc::QoS;
use std::time::Duration;

/// Returns the stream of messages received on a specific topic.
///
/// To ease testing, the errors are returned as messages.
pub async fn messages_published_on(mqtt_port: u16, topic: &str) -> UnboundedReceiver<String> {
    let (mut sender, recv) = futures::channel::mpsc::unbounded();

    // One can have a connection error if this is called just after the broker starts
    // So try to subscribe again after a first error
    let mut con = TestCon::new(mqtt_port);
    let mut retry = 1;
    loop {
        match con.subscribe(topic, QoS::AtLeastOnce).await {
            Ok(()) => break,
            Err(_) if retry > 0 => {
                tokio::time::sleep(Duration::from_secs(1)).await;
                retry -= 1;
                continue;
            }
            Err(err) => {
                let msg = format!("Error: {:?}", err);
                sender.send(msg).await.unwrap();
                return recv;
            }
        }
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
        let actual_msg = messages.next().with_timeout(timeout).await;
        assert_eq!(actual_msg, Ok(Some(expected_msg.to_string())));
    }
}

/// Pull the received `messages` until all the `expected` messages have been received or timeout reached.
///
/// A message is expected if containing one of the `expected` strings.
/// Returns early on `timeout` while waiting for the next message.
pub async fn assert_received_all_expected<T>(
    messages: &mut UnboundedReceiver<String>,
    timeout: Duration,
    expected: T,
) where
    T: IntoIterator,
    T::Item: ToString,
{
    let mut expected = expected
        .into_iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    let mut received = Vec::new();

    while let Ok(Some(msg)) = messages.next().with_timeout(timeout).await {
        expected.retain(|expected_msg| !msg.contains(expected_msg));
        received.push(msg);
        if expected.is_empty() {
            return;
        }
    }

    assert!(
        expected.is_empty(),
        "Didn't receive all expected messages: {expected:?}\n Received: {received:?}",
    );
}

/// Publish a message
///
/// Return only when the message has been acknowledged.
pub async fn publish(
    mqtt_port: u16,
    topic: &str,
    payload: &str,
    qos: QoS,
    retain: bool,
) -> Result<(), anyhow::Error> {
    let mut con = TestCon::new(mqtt_port);

    con.publish(topic, qos, retain, payload).await
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
    con.publish(pub_topic, QoS::AtLeastOnce, false, pub_message)
        .await
        .ok()?;
    match tokio::time::timeout(timeout, con.next_message()).await {
        // One collapse both timeout and error to None
        Err(_) | Ok(Err(_)) => None,
        Ok(Ok(x)) => Some(x),
    }
}

pub async fn map_messages_loop<F>(mqtt_port: u16, func: F)
where
    F: Send + Sync + Fn((String, String)) -> Vec<(String, String)>,
{
    let mut con = TestCon::new(mqtt_port);
    con.subscribe("#", QoS::AtLeastOnce)
        .await
        .expect("Fail to subscribe on #");

    loop {
        if let Ok(message) = con.next_topic_payload().await {
            for (topic, response) in func(message).iter() {
                let _ = con.publish(topic, QoS::AtLeastOnce, false, response).await;
            }
        }
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
            if let Event::Incoming(Packet::SubAck(_)) = self.eventloop.poll().await? {
                return Ok(());
            }
        }
    }

    pub async fn publish(
        &mut self,
        topic: &str,
        qos: QoS,
        retain: bool,
        payload: &str,
    ) -> Result<(), anyhow::Error> {
        self.client.publish(topic, qos, retain, payload).await?;

        loop {
            if let Event::Incoming(Packet::PubAck(_)) = self.eventloop.poll().await? {
                return Ok(());
            }
        }
    }

    pub async fn forward_received_messages(&mut self, mut sender: UnboundedSender<String>) {
        loop {
            match self.eventloop.poll().await {
                Ok(Event::Incoming(Packet::Publish(response))) => {
                    let msg = std::str::from_utf8(&response.payload)
                        .unwrap_or("Error: non-utf8-payload")
                        .to_string();
                    if sender.send(msg).await.is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let msg = format!("Error: {:?}", err);
                    let _ = sender.send(msg).await;
                    break;
                }
                _ => {}
            }
        }
        let _ = self.client.disconnect().await;
    }

    pub async fn next_message(&mut self) -> Result<String, anyhow::Error> {
        loop {
            if let Event::Incoming(Packet::Publish(packet)) = self.eventloop.poll().await? {
                let msg = std::str::from_utf8(&packet.payload)
                    .unwrap_or("Error: non-utf8-payload")
                    .to_string();
                return Ok(msg);
            }
        }
    }

    pub async fn next_topic_payload(&mut self) -> Result<(String, String), anyhow::Error> {
        loop {
            if let Event::Incoming(Packet::Publish(packet)) = self.eventloop.poll().await? {
                let topic = packet.topic.clone();
                let msg = std::str::from_utf8(&packet.payload)
                    .unwrap_or("Error: non-utf8-payload")
                    .to_string();
                return Ok((topic, msg));
            }
        }
    }
}
