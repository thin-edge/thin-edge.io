use crate::cli::mqtt::MqttError;
use crate::command::Command;
use futures::future::FutureExt;
use mqtt_client::{Client, Message, MqttClient, MqttClientError, QoS, Topic};
use std::time::Duration;
use tokio::{pin, select};

pub struct MqttPublishCommand {
    pub topic: Topic,
    pub message: String,
    pub qos: QoS,
    pub mqtt_config: mqtt_client::Config,
    pub client_id: String,
    pub disconnect_timeout: Duration,
}

impl Command for MqttPublishCommand {
    fn description(&self) -> String {
        format!(
            "publish the message \"{}\" on the topic \"{}\" with QoS \"{:?}\".",
            self.message, self.topic.name, self.qos
        )
    }

    fn execute(&self) -> anyhow::Result<()> {
        Ok(publish(self)?)
    }
}

#[tokio::main]
async fn publish(cmd: &MqttPublishCommand) -> Result<(), MqttError> {
    let mut mqtt = cmd
        .mqtt_config
        .clone()
        .clean_session()
        .connect(cmd.client_id.as_str())
        .await?;

    let message = Message::new(&cmd.topic, cmd.message.as_str()).qos(cmd.qos);
    let res = try_publish(&mut mqtt, message).await;

    // In case we don't have a connection, disconnect might block until there is a connection,
    // therefore timeout.
    let _ = tokio::time::timeout(cmd.disconnect_timeout, mqtt.disconnect()).await;
    res
}

async fn try_publish(mqtt: &mut Client, msg: Message) -> Result<(), MqttError> {
    let mut errors = mqtt.subscribe_errors();

    let fut = async move {
        match mqtt.publish(msg).await {
            Ok(()) => {
                // Wait until all messages have been published.
                let () = mqtt.all_completed().await;
                Ok(())
            }
            Err(err) => Err(err),
        }
    };
    pin!(fut);

    loop {
        select! {
            error = errors.next().fuse() => {
                if let Some(err) = error {
                    if let MqttClientError::ConnectionError(..) = *err {
                        return Err(MqttError::ServerError(err.to_string()));
                    }
                }
            }

            result = &mut fut => {
                return result.map_err(Into::into);
            }
        }
    }
}
