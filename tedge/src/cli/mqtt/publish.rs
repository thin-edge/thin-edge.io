use crate::cli::mqtt::MqttError;
use crate::command::{Command, ExecutionContext};
use futures::future::FutureExt;
use mqtt_client::{Client, Message, QoS, Topic};
use std::time::Duration;
use tokio::select;

pub struct MqttPublishCommand {
    pub topic: String,
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
            self.message, self.topic, self.qos
        )
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        Ok(publish(self)?)
    }
}

#[tokio::main]
async fn publish(cmd: &MqttPublishCommand) -> Result<(), MqttError> {
    let mut mqtt = cmd
        .mqtt_config
        .clone()
        .connect(cmd.client_id.as_str())
        .await?;

    let tpc = Topic::new(cmd.topic.as_str())?;
    let message = Message::new(&tpc, cmd.message.as_str()).qos(cmd.qos);

    let res = try_publish(&mut mqtt, message).await;

    // In case we don't have a connection, disconnect might block until there is a connection,
    // therefore timeout.
    let _ = tokio::time::timeout(cmd.disconnect_timeout, mqtt.disconnect()).await;
    res
}

async fn try_publish(mqtt: &mut Client, msg: Message) -> Result<(), MqttError> {
    let mut errors = mqtt.subscribe_errors();

    // This requires 2 awaits as publish_with_ack returns a future which returns a future.
    let ack = async {
        match mqtt.publish_with_ack(msg).await {
            Ok(fut) => fut.await,
            Err(err) => Err(err),
        }
    };

    select! {
        error = errors.next().fuse() => {
            if let Some(err) = error {
                if let mqtt_client::Error::ConnectionError(..) = *err {
                    return Err(MqttError::ServerError(err.to_string()));
                }
            }
        }

        result = ack.fuse() => {
            result?
        }
    }

    Ok(())
}
