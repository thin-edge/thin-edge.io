use async_trait::async_trait;
use tedge_actors::Actor;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::health::ServiceHealthTopic;
use tedge_mqtt_ext::MqttMessage;

pub struct HealthMonitorActor {
    // TODO(marcel): move this
    service_registration_message: Option<MqttMessage>,
    health_topic: ServiceHealthTopic,
    messages: SimpleMessageBox<MqttMessage, MqttMessage>,
    // Unrelated to the health of the service, but published here to follow its registration
    control_capability: Option<MqttMessage>,
}

impl HealthMonitorActor {
    pub fn new(
        service_registration_message: Option<MqttMessage>,
        health_topic: ServiceHealthTopic,
        messages: SimpleMessageBox<MqttMessage, MqttMessage>,
        control_capability: Option<MqttMessage>,
    ) -> Self {
        Self {
            service_registration_message,
            health_topic,
            messages,
            control_capability,
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

    async fn run(mut self) -> Result<(), RuntimeError> {
        if let Some(registration_message) = &self.service_registration_message {
            self.messages.send(registration_message.clone()).await?;
        }

        // The capability should be sent after the service registration
        if let Some(control_capability) = &self.control_capability {
            self.messages.send(control_capability.clone()).await?;
        }

        self.messages.send(self.up_health_status()).await?;

        while let Some(_message) = self.messages.recv().await {
            self.messages.send(self.up_health_status()).await?;
        }
        Ok(())
    }
}
