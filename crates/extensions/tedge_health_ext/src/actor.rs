use async_trait::async_trait;
use tedge_actors::Actor;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::health::ServiceHealthTopic;
use tedge_mqtt_ext::MqttMessage;

pub struct HealthMonitorActor {
    health_topic: ServiceHealthTopic,
    messages: SimpleMessageBox<MqttMessage, MqttMessage>,
}

impl HealthMonitorActor {
    pub fn new(
        health_topic: ServiceHealthTopic,
        messages: SimpleMessageBox<MqttMessage, MqttMessage>,
    ) -> Self {
        Self {
            health_topic,
            messages,
        }
    }

    pub fn up_health_status(&self) -> MqttMessage {
        self.health_topic.up_message()
    }

    pub fn down_health_status(&self) -> MqttMessage {
        self.health_topic.down_message()
    }
}

#[async_trait]
impl Actor for HealthMonitorActor {
    fn name(&self) -> &str {
        "HealthMonitorActor"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        self.messages.send(self.up_health_status()).await?;
        while let Some(_message) = self.messages.recv().await {
            {
                self.messages.send(self.up_health_status()).await?;
            }
        }
        Ok(())
    }
}
