use crate::cli::mqtt::MqttError;
use crate::command::Command;
use futures::future::FutureExt;
use mqtt_client::{Client, Message, MqttClient, QoS, TopicFilter};
use tedge_utils::signals;
use tokio::{io::AsyncWriteExt, select};

pub struct MqttSubscribeCommand {
    pub topic: String,
    pub qos: QoS,
    pub hide_topic: bool,
    pub mqtt_config: mqtt_client::Config,
    pub client_id: String,
}

impl Command for MqttSubscribeCommand {
    fn description(&self) -> String {
        format!(
            "subscribe the topic \"{}\" with QoS \"{:?}\".",
            self.topic, self.qos
        )
    }

    fn execute(&self) -> anyhow::Result<()> {
        Ok(subscribe(self)?)
    }
}

#[tokio::main]
async fn subscribe(cmd: &MqttSubscribeCommand) -> Result<(), MqttError> {
    let heart_beat = std::time::Duration::from_secs(5);
    let config = cmd
        .mqtt_config
        .clone()
        .clean_session()
        .with_keep_alive(heart_beat);
    let filter = TopicFilter::new(cmd.topic.as_str())?.qos(cmd.qos);
    let mut first_connection = true;

    loop {
        let mqtt = Client::connect(cmd.client_id.as_str(), &config).await?;
        let mut errors = mqtt.subscribe_errors();
        let mut messages = mqtt.subscribe(filter.clone()).await?;

        if !first_connection {
            async_println("INFO: Reconnecting").await?;
        }
        loop {
            select! {
                maybe_error = errors.next().fuse() => {
                    if let Some(error) = maybe_error {
                        if error.to_string().contains("MQTT connection error: I/O: Connection refused (os error 111)") {
                            return Err(MqttError::ServerError(error.to_string()));
                        }
                        async_println(&format!("ERROR: {:?}", error)).await?;
                        async_println("INFO: Disconnecting").await?;
                        let _ = tokio::time::timeout(heart_beat * 2, mqtt.disconnect()).await;
                        first_connection = false;
                        break;
                    }
                }

                _signal = signals::interrupt().fuse() => {
                    println!("Received SIGINT.");
                    return Ok(());
                }

                maybe_message = messages.next().fuse() => {
                    match maybe_message {
                        Some(message) =>  handle_message(message, cmd.hide_topic).await?,
                        None => break
                    }
                }
            }
        }
    }
}

async fn async_println(s: &str) -> Result<(), MqttError> {
    let mut stdout = tokio::io::stdout();
    stdout.write_all(s.as_bytes()).await?;
    stdout.write_all(b"\n").await?;
    Ok(())
}

async fn handle_message(message: Message, hide_topic: bool) -> Result<(), MqttError> {
    let payload = message.payload_str()?;
    if hide_topic {
        async_println(&payload).await?;
    } else {
        let s = format!("[{}] {}", message.topic.name, payload);
        async_println(&s).await?;
    }

    Ok(())
}
