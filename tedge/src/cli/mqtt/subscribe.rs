use crate::cli::mqtt::MqttError;
use crate::command::{Command, ExecutionContext};
use crate::utils::signals;
use futures::future::FutureExt;
use mqtt_client::{Client, Message, MqttClient, QoS, TopicFilter};
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

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        Ok(subscribe(self)?)
    }
}

#[tokio::main]
async fn subscribe(cmd: &MqttSubscribeCommand) -> Result<(), MqttError> {
    let config = cmd.mqtt_config.clone().clean_session();
    let mqtt = Client::connect(cmd.client_id.as_str(), &config).await?;
    let filter = TopicFilter::new(cmd.topic.as_str())?.qos(cmd.qos);

    let mut errors = mqtt.subscribe_errors();
    let mut messages = mqtt.subscribe(filter).await?;

    loop {
        select! {
            error = errors.next().fuse() => {
                if let Some(err) = error {
                    if err.to_string().contains("MQTT connection error: I/O: Connection refused (os error 111)") {
                        return Err(MqttError::ServerError(err.to_string()));
                    }
                }
            }

            _signal = signals::interrupt().fuse() => {
                println!("Received SIGINT.");
                break;
            }

            maybe_message = messages.next().fuse() => {
                match maybe_message {
                    Some(message) =>  handle_message(message, cmd.hide_topic).await?,
                    None => break
                 }
            }
        }
    }

    Ok(())
}

async fn async_println(s: &str) -> Result<(), MqttError> {
    let mut stdout = tokio::io::stdout();
    stdout.write_all(s.as_bytes()).await?;
    stdout.write_all(b"\n").await?;
    Ok(())
}

async fn handle_message(message: Message, hide_topic: bool) -> Result<(), MqttError> {
    if hide_topic {
        let s = std::str::from_utf8(message.payload_trimmed())?;
        async_println(&s).await?;
    } else {
        let s = format!(
            "[{}] {}",
            message.topic.name,
            std::str::from_utf8(message.payload_trimmed())?
        );
        async_println(&s).await?;
    }

    Ok(())
}
