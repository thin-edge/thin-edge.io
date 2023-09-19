mod actor;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use actor::HealthMonitorActor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::MessageSink;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceConsumer;
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::health::ServiceHealthTopic;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::mqtt_topics::ServiceTopicId;
use tedge_mqtt_ext::MqttConfig;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;

pub struct HealthMonitorBuilder {
    service_health_topic: ServiceHealthTopic,
    box_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
}

impl HealthMonitorBuilder {
    /// Creates a HealthMonitorBuilder that creates a HealthMonitorActor with
    /// old topic scheme.
    pub fn new(
        service_name: &str,
        mqtt: &mut (impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter> + AsMut<MqttConfig>),
    ) -> Self {
        // Connect this actor to MQTT
        let subscriptions = vec![
            "tedge/health-check",
            &format!("tedge/health-check/{service_name}"),
        ]
        .try_into()
        .expect("Failed to create the HealthMonitorActor topic filter");

        let service_health_topic =
            ServiceHealthTopic::from_old_topic(format!("tedge/health/{service_name}")).unwrap();

        let mut box_builder = SimpleMessageBoxBuilder::new(service_name, 16);
        box_builder
            .set_request_sender(mqtt.connect_consumer(subscriptions, box_builder.get_sender()));

        let builder = HealthMonitorBuilder {
            service_health_topic,
            box_builder,
        };

        // Update the MQTT config
        *mqtt.as_mut() = builder.set_init_and_last_will(mqtt.as_mut().clone());

        builder
    }

    /// Creates a HealthMonitorBuilder that creates a HealthMonitorActor with
    /// a new topic scheme.
    pub fn from_service_topic_id(
        service: ServiceTopicId,
        mqtt: &mut (impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter> + AsMut<MqttConfig>),
        // TODO: pass it less annoying way
        mqtt_topic_root: Arc<str>,
    ) -> Self {
        let health_topic = ServiceHealthTopic::new(service.clone());

        let mut box_builder = SimpleMessageBoxBuilder::new(service.as_str(), 16);

        // passed service is in default scheme
        let device_topic_id = service.to_device_topic_id().unwrap();

        let mqtt_schema = MqttSchema::with_root(mqtt_topic_root.to_string());
        let subscriptions = vec![
            mqtt_schema.topic_for(
                service.clone().entity(),
                &Channel::Command {
                    operation: OperationType::HealthCheck,
                    cmd_id: None,
                },
            ),
            mqtt_schema.topic_for(
                device_topic_id.entity(),
                &Channel::Command {
                    operation: OperationType::HealthCheck,
                    cmd_id: None,
                },
            ),
        ]
        .into_iter()
        .map(|t| t.into())
        .collect::<TopicFilter>();

        box_builder
            .set_request_sender(mqtt.connect_consumer(subscriptions, box_builder.get_sender()));

        let builder = HealthMonitorBuilder {
            service_health_topic: health_topic,
            box_builder,
        };

        // Update the MQTT config
        *mqtt.as_mut() = builder.set_init_and_last_will(mqtt.as_mut().clone());

        builder
    }

    fn set_init_and_last_will(&self, config: MqttConfig) -> MqttConfig {
        let name = self.service_health_topic.to_owned();
        let _name = name.clone();
        config
            .with_initial_message(move || _name.up_message())
            .with_last_will_message(name.down_message())
    }
}

impl RuntimeRequestSink for HealthMonitorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.box_builder.get_signal_sender())
    }
}

impl Builder<HealthMonitorActor> for HealthMonitorBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<HealthMonitorActor, Self::Error> {
        let message_box = self.box_builder.build();
        let actor = HealthMonitorActor::new(self.service_health_topic, message_box);

        Ok(actor)
    }
}
