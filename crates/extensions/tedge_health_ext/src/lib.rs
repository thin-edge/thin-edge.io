mod actor;

#[cfg(test)]
mod tests;

use actor::HealthMonitorActor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::entity_store::EntityType;
use tedge_api::health::ServiceHealthTopic;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::mqtt_topics::Service;
use tedge_config::TEdgeConfigReaderService;
use tedge_mqtt_ext::MqttConfig;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;

pub struct HealthMonitorBuilder {
    registration_message: Option<MqttMessage>,
    health_topic: ServiceHealthTopic,
    box_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
}

impl HealthMonitorBuilder {
    /// Creates a HealthMonitorBuilder that creates a HealthMonitorActor with
    /// a new topic scheme.
    pub fn from_service_topic_id(
        service: Service,
        mqtt: &mut (impl MessageSource<MqttMessage, TopicFilter>
                  + MessageSink<MqttMessage>
                  + AsMut<MqttConfig>),
        // TODO: pass it less annoying way
        mqtt_schema: &MqttSchema,
        service_config: &TEdgeConfigReaderService,
    ) -> Self {
        let mut service_type = service_config.ty.as_str();
        let time_format = service_config.timestamp_format;
        let service_topic_id = &service.service_topic_id;

        let mut box_builder = SimpleMessageBoxBuilder::new(service_topic_id.as_str(), 16);

        let subscriptions: TopicFilter = [
            mqtt_schema
                .topic_for(
                    service.service_topic_id.entity(),
                    &Channel::Command {
                        operation: OperationType::Health,
                        cmd_id: "check".to_string(),
                    },
                )
                .into(),
            mqtt_schema
                .topic_for(
                    service.device_topic_id.entity(),
                    &Channel::Command {
                        operation: OperationType::Health,
                        cmd_id: "check".to_string(),
                    },
                )
                .into(),
        ]
        .into_iter()
        .collect();

        mqtt.connect_sink(subscriptions, &box_builder);
        box_builder.connect_sink(NoConfig, mqtt);

        if service_type.is_empty() {
            service_type = "service"
        }

        let registration_message = EntityRegistrationMessage {
            topic_id: service_topic_id.entity().clone(),
            external_id: None,
            r#type: EntityType::Service,
            parent: Some(service.device_topic_id.entity().clone()),
            other: serde_json::json!({ "type": service_type })
                .as_object()
                .unwrap()
                .to_owned(),
        };
        let registration_message = registration_message.to_mqtt_message(mqtt_schema);

        let health_topic =
            ServiceHealthTopic::from_new_topic(service_topic_id, mqtt_schema, time_format);

        let builder = HealthMonitorBuilder {
            health_topic,
            registration_message: Some(registration_message),
            box_builder,
        };

        // Update the MQTT config

        // XXX: if the same MqttActorBuilder is used in different actors, then
        // this will override init messages that may have been set by other
        // actors!
        *mqtt.as_mut() = builder.set_init_and_last_will(mqtt.as_mut().clone());

        builder
    }

    fn set_init_and_last_will(&self, config: MqttConfig) -> MqttConfig {
        let name = self.health_topic.to_owned();
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

        let actor =
            HealthMonitorActor::new(self.registration_message, self.health_topic, message_box);

        Ok(actor)
    }
}
