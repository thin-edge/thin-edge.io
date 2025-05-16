use crate::command::Command;
use crate::log::MaybeFancy;
use camino::Utf8PathBuf;
use mqtt_channel::MqttMessage;
use mqtt_channel::PubChannel;
use mqtt_channel::Topic;
use tedge_config::tedge_toml::MqttAuthClientConfig;
use tedge_config::TEdgeConfig;
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
    pub ca_file: Option<Utf8PathBuf>,
    pub ca_dir: Option<Utf8PathBuf>,
    pub client_auth_config: Option<MqttAuthClientConfig>,
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
        Ok(publish(self).await?)
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

    if let Some(ca_file) = &cmd.ca_file {
        config.with_cafile(ca_file)?;
    }
    if let Some(ca_dir) = &cmd.ca_dir {
        config.with_cadir(ca_dir)?;
    }
    if let Some(client_auth) = cmd.client_auth_config.as_ref() {
        config.with_client_auth(&client_auth.cert_file, &client_auth.key_file)?;
    }

    let mut mqtt = mqtt_channel::Connection::new(&config).await?;
    let mut signals = tedge_utils::signals::TermSignals::new(None);

    let message = MqttMessage::new(&cmd.topic, cmd.message.clone())
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
