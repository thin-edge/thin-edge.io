pub mod test_mqtt_server;

use rumqttc::{MqttOptions, AsyncClient, Event, Packet, Outgoing, Incoming};
use rumqttc::QoS::AtLeastOnce;

pub async fn received_on_published(mqtt_port: u16, pub_topic: &str, pub_message: &str, sub_topic: &str, timeout_sec: u16) -> Result<String, anyhow::Error> {
    let mut options = MqttOptions::new("test", "localhost", mqtt_port);
    options.set_keep_alive(timeout_sec);
    options.set_clean_session(true);

    let (client, mut eventloop) = AsyncClient::new(options, 10);
    client.subscribe(sub_topic, AtLeastOnce).await?;

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client.publish(
                    pub_topic,
                    AtLeastOnce,
                    false,
                    pub_message,
                ).await?;
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
