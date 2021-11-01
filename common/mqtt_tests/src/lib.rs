pub mod message_logger;
pub mod test_mqtt_server;
pub mod with_timeout;

use rumqttc::QoS::AtLeastOnce;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, Outgoing, Packet};

/// Publish the `pub_message` on the `pub_topic` only when ready to receive a message on `sub_topic`.
///
/// 1. Subscribe to the `sub_topic`,
/// 2. Wait for the acknowledgment of the subscription
/// 3  Publish the `pub_message` on the `pub_topic`,
/// 4. Return the first received message
/// 5. or give up after `timeout_sec` secondes.
pub async fn received_on_published(
    mqtt_port: u16,
    pub_topic: &str,
    pub_message: &str,
    sub_topic: &str,
    timeout_sec: u16,
) -> Result<String, anyhow::Error> {
    let mut options = MqttOptions::new("test", "localhost", mqtt_port);
    options.set_keep_alive(timeout_sec);
    options.set_clean_session(true);

    let (client, mut eventloop) = AsyncClient::new(options, 10);
    client.subscribe(sub_topic, AtLeastOnce).await?;

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client
                    .publish(pub_topic, AtLeastOnce, false, pub_message)
                    .await?;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                // We got a response
                client.disconnect().await?;
                return Ok(std::str::from_utf8(&response.payload)?.to_string());
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                client.disconnect().await?;
                return Err(anyhow::anyhow!("Timeout"));
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
}
