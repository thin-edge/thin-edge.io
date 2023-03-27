use async_trait::async_trait;
use tedge_actors::Actor;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::SimpleMessageBox;
use tedge_mqtt_ext::MqttMessage;

use tedge_api::health::health_status_down_message;
use tedge_api::health::health_status_up_message;

pub struct HealthMonitorActor {
    daemon_name: String,
}

impl HealthMonitorActor {
    pub fn new(daemon_name: String) -> Self {
        Self { daemon_name }
    }

    pub fn up_health_status(&self) -> MqttMessage {
        health_status_up_message(&self.daemon_name)
    }

    pub fn down_health_status(&self) -> MqttMessage {
        health_status_down_message(&self.daemon_name)
    }
}

#[async_trait]
impl Actor for HealthMonitorActor {
    type MessageBox = SimpleMessageBox<MqttMessage, MqttMessage>;

    fn name(&self) -> &str {
        "HealthMonitorActor"
    }

    async fn run(mut self, mut messages: Self::MessageBox) -> Result<(), RuntimeError> {
        messages.send(self.up_health_status()).await?;
        while let Some(_message) = messages.recv().await {
            {
                messages.send(self.up_health_status()).await?;
            }
        }
        Ok(())
    }
}
