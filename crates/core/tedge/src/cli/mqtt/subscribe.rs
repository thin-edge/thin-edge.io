use crate::command::CommandAsync;
use crate::log::MaybeFancy;
use camino::Utf8PathBuf;
use mqtt_channel::QoS;
use mqtt_channel::StreamExt;
use mqtt_channel::TopicFilter;
use std::time::Duration;
use tedge_config::tedge_toml::MqttAuthClientConfig;
use tokio::io::AsyncWriteExt;
use tracing::info;

const DEFAULT_QUEUE_CAPACITY: usize = 10;
use super::MAX_PACKET_SIZE;
use crate::error;

pub struct MqttSubscribeCommand {
    pub host: String,
    pub port: u16,
    pub topic: SimpleTopicFilter,
    pub qos: QoS,
    pub hide_topic: bool,
    pub client_id: String,
    pub ca_file: Option<Utf8PathBuf>,
    pub ca_dir: Option<Utf8PathBuf>,
    pub client_auth_config: Option<MqttAuthClientConfig>,
    pub duration: Option<Duration>,
    pub count: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct SimpleTopicFilter(String);

#[async_trait::async_trait]
impl CommandAsync for MqttSubscribeCommand {
    fn description(&self) -> String {
        format!(
            "subscribe to the topic \"{:?}\" with QoS \"{:?}\".",
            self.topic.pattern(),
            self.qos
        )
    }

    async fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        Ok(subscribe(self).await?)
    }
}

async fn subscribe(cmd: &MqttSubscribeCommand) -> Result<(), anyhow::Error> {
    let topic = TopicFilter::new(cmd.topic.pattern())?.with_qos(cmd.qos);

    let mut config = mqtt_channel::Config::default()
        .with_host(cmd.host.clone())
        .with_port(cmd.port)
        .with_session_name(cmd.client_id.clone())
        .with_clean_session(true)
        .with_subscriptions(topic)
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
    let mut signals = tedge_utils::signals::TermSignals::new(cmd.duration);
    let mut n_messages = 0;
    let mut stdout = tokio::io::stdout();
    loop {
        let message = match signals.might_interrupt(mqtt.received.next()).await {
            Ok(Some(message)) => message,
            Ok(None) => break,
            Err(signal) => {
                info!("{signal:?}");
                break;
            }
        };

        match message.payload_str() {
            Ok(payload) => {
                let line = if cmd.hide_topic {
                    format!("{payload}\n")
                } else {
                    format!("[{}] {payload}\n", &message.topic)
                };
                let _ = stdout.write_all(line.as_bytes()).await;
                let _ = stdout.flush().await;
                n_messages += 1;
                if matches!(cmd.count, Some(count) if count > 0 && n_messages >= count) {
                    info!("Received {n_messages} message/s");
                    break;
                }
            }
            Err(err) => error!("{err}"),
        }
    }

    mqtt.published.close_channel();
    mqtt.pub_done.await?;
    Ok(())
}

// Using TopicFilter for `tedge sub` would lead to complicate code for nothing
// because a TopicFilter is a set of patterns while `tedge sub` uses a single pattern.
impl SimpleTopicFilter {
    pub fn new(pattern: &str) -> Result<SimpleTopicFilter, mqtt_channel::MqttError> {
        let _ = TopicFilter::new(pattern)?;
        Ok(SimpleTopicFilter(pattern.to_string()))
    }

    pub fn pattern(&self) -> &str {
        self.0.as_str()
    }
}
