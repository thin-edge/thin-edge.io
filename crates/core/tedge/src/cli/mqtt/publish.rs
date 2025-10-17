use crate::command::Command;
use crate::log::MaybeFancy;
use base64::prelude::*;
use mqtt_channel::MqttMessage;
use mqtt_channel::PubChannel;
use mqtt_channel::Topic;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeMqttClientAuthConfig;
use tracing::info;

const DEFAULT_QUEUE_CAPACITY: usize = 10;
use super::MAX_PACKET_SIZE;

pub struct MqttPublishCommand {
    pub host: String,
    pub port: u16,
    pub topic: Topic,
    pub message: String,
    pub qos: mqtt_channel::QoS,
    pub client_id: String,
    pub retain: bool,
    pub base64: bool,
    pub auth_config: TEdgeMqttClientAuthConfig,
    pub count: u32,
    pub sleep: std::time::Duration,
}

#[async_trait::async_trait]
impl Command for MqttPublishCommand {
    fn description(&self) -> String {
        format!(
            "publish the message \"{}\" on the topic \"{}\" with QoS \"{:?}\".",
            self.message,
            self.topic.as_ref(),
            self.qos
        )
    }

    async fn execute(&self, _: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let mut i = 0;
        loop {
            publish(self).await?;
            i += 1;
            if i == self.count {
                return Ok(());
            }
            tokio::time::sleep(self.sleep).await;
        }
    }
}

async fn publish(cmd: &MqttPublishCommand) -> Result<(), anyhow::Error> {
    let mut config = mqtt_channel::Config::default()
        .with_host(cmd.host.clone())
        .with_port(cmd.port)
        .with_session_name(cmd.client_id.clone())
        .with_clean_session(true)
        .with_max_packet_size(MAX_PACKET_SIZE)
        .with_queue_capacity(DEFAULT_QUEUE_CAPACITY);

    config.with_client_auth(cmd.auth_config.clone().try_into()?)?;

    let mut mqtt = mqtt_channel::Connection::new(&config).await?;
    let mut signals = tedge_utils::signals::TermSignals::new(None);

    let payload = if cmd.base64 {
        BASE64_STANDARD.decode(cmd.message.as_bytes())?
    } else {
        cmd.message.clone().into_bytes()
    };

    let message = MqttMessage::new(&cmd.topic, payload)
        .with_qos(cmd.qos)
        .with_retain_flag(cmd.retain);

    match signals
        .might_interrupt(mqtt.published.publish(message))
        .await
    {
        Ok(Ok(())) => (),
        Ok(err) => err?,
        Err(signal) => info!(target: "MQTT", "{signal:?}"),
    }
    mqtt.close().await;

    Ok(())
}
