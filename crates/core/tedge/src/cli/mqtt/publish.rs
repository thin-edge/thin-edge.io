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

fn build_config(cmd: &MqttPublishCommand) -> Result<mqtt_channel::Config, anyhow::Error> {
    let mut config = mqtt_channel::Config::default()
        .with_host(cmd.host.clone())
        .with_port(cmd.port)
        .with_session_prefix(cmd.client_id.clone())
        .with_clean_session(true)
        .with_max_packet_size(MAX_PACKET_SIZE)
        .with_queue_capacity(DEFAULT_QUEUE_CAPACITY);
    config.with_client_auth(cmd.auth_config.clone().try_into()?)?;
    Ok(config)
}

async fn publish(cmd: &MqttPublishCommand) -> Result<(), anyhow::Error> {
    let config = build_config(cmd)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_channel::Topic;
    use tedge_config::TEdgeMqttClientAuthConfig;

    #[test]
    fn two_publishers_with_same_client_id_use_different_session_names() {
        // In containerised environments, multiple hosts can share the same PID,
        // meaning cli.rs may provide the same client_id to concurrent publishers.
        let client_id = "tedge-pub-1";
        let cmd1 = pub_command_with_client_id(client_id);
        let cmd2 = pub_command_with_client_id(client_id);

        let session1 = build_config(&cmd1).unwrap().session_name;
        let session2 = build_config(&cmd2).unwrap().session_name;

        assert_ne!(
            session1, session2,
            "Concurrent publishers with the same client_id must get unique MQTT session names"
        );
    }

    fn pub_command_with_client_id(client_id: &str) -> MqttPublishCommand {
        MqttPublishCommand {
            host: "localhost".into(),
            port: 1883,
            topic: Topic::new("test/topic").unwrap(),
            message: "hello".into(),
            qos: mqtt_channel::QoS::AtMostOnce,
            client_id: client_id.to_string(),
            retain: false,
            base64: false,
            auth_config: TEdgeMqttClientAuthConfig::default(),
            count: 1,
            sleep: std::time::Duration::ZERO,
        }
    }
}
