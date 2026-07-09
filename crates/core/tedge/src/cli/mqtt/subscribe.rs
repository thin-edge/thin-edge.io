use crate::command::Command;
use crate::log::MaybeFancy;
use base64::prelude::*;
use mqtt_channel::QoS;
use mqtt_channel::StreamExt;
use mqtt_channel::TopicFilter;
use std::time::Duration;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeMqttClientAuthConfig;
use tokio::io::AsyncWriteExt;
use tracing::info;

const DEFAULT_QUEUE_CAPACITY: usize = 10;
use super::MAX_PACKET_SIZE;

pub struct MqttSubscribeCommand {
    pub host: String,
    pub port: u16,
    pub topic: SimpleTopicFilter,
    pub qos: QoS,
    pub hide_topic: bool,
    pub base64: bool,
    pub client_id: String,
    pub auth_config: TEdgeMqttClientAuthConfig,
    pub duration: Option<Duration>,
    pub count: Option<u32>,
    pub retained_only: bool,
}

#[derive(Clone, Debug)]
pub struct SimpleTopicFilter(String);

#[async_trait::async_trait]
impl Command for MqttSubscribeCommand {
    fn description(&self) -> String {
        format!(
            "subscribe to the topic \"{:?}\" with QoS \"{:?}\".",
            self.topic.pattern(),
            self.qos
        )
    }

    async fn execute(&self, _: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        Ok(subscribe(self).await?)
    }
}

fn build_config(cmd: &MqttSubscribeCommand) -> Result<mqtt_channel::Config, anyhow::Error> {
    let topic = TopicFilter::new(cmd.topic.pattern())?.with_qos(cmd.qos);
    let mut config = mqtt_channel::Config::default()
        .with_host(cmd.host.clone())
        .with_port(cmd.port)
        .with_session_prefix(cmd.client_id.clone())
        .with_clean_session(true)
        .with_subscriptions(topic)
        .with_max_packet_size(MAX_PACKET_SIZE)
        .with_queue_capacity(DEFAULT_QUEUE_CAPACITY);
    config.with_client_auth(cmd.auth_config.clone().try_into()?)?;
    Ok(config)
}

async fn subscribe(cmd: &MqttSubscribeCommand) -> Result<(), anyhow::Error> {
    let config = build_config(cmd)?;

    let mut mqtt = mqtt_channel::Connection::new(&config).await?;
    let mut signals = tedge_utils::signals::TermSignals::new(cmd.duration);
    let mut n_messages = 0;
    let mut stdout = tokio::io::stdout();
    loop {
        let message = match signals.might_interrupt(mqtt.received.next()).await {
            Ok(Some(message)) => message,
            Ok(None) => break,
            Err(signal) => {
                info!(target: "MQTT", "{signal:?}");
                break;
            }
        };

        if cmd.retained_only && !message.retain {
            info!(target: "MQTT", topic = message.topic.name, "Received first non-retained message.");
            break;
        }

        let payload = if cmd.base64 {
            BASE64_STANDARD.encode(message.payload_bytes())
        } else {
            match message.payload_str() {
                Ok(payload_str) => payload_str.to_string(),
                Err(_) => format!(
                    "<ERR=NON-UTF8> {}",
                    BASE64_STANDARD.encode(message.payload_bytes())
                ),
            }
        };

        let line = if cmd.hide_topic {
            format!("{payload}\n")
        } else {
            format!("[{}] {payload}\n", message.topic)
        };
        let _ = stdout.write_all(line.as_bytes()).await;
        let _ = stdout.flush().await;

        n_messages += 1;
        if matches!(cmd.count, Some(count) if count > 0 && n_messages >= count) {
            info!(target: "MQTT", "Received {n_messages} message/s");
            break;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_config::TEdgeMqttClientAuthConfig;

    #[test]
    fn two_subscribers_with_same_client_id_use_different_session_names() {
        // In containerised environments, multiple hosts can share the same PID,
        // meaning cli.rs may provide the same client_id to concurrent subscribers.
        let client_id = "tedge-sub-1";
        let cmd1 = sub_command_with_client_id(client_id);
        let cmd2 = sub_command_with_client_id(client_id);

        let session1 = build_config(&cmd1).unwrap().session_name;
        let session2 = build_config(&cmd2).unwrap().session_name;

        assert_ne!(
            session1, session2,
            "Concurrent subscribers with the same client_id must get unique MQTT session names"
        );
    }

    fn sub_command_with_client_id(client_id: &str) -> MqttSubscribeCommand {
        MqttSubscribeCommand {
            host: "localhost".into(),
            port: 1883,
            topic: SimpleTopicFilter::new("test/#").unwrap(),
            qos: QoS::AtMostOnce,
            hide_topic: false,
            base64: false,
            client_id: client_id.to_string(),
            auth_config: TEdgeMqttClientAuthConfig::default(),
            duration: None,
            count: None,
            retained_only: false,
        }
    }
}
