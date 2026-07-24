//! A shared actor that publishes a component's own exposable configuration values as retained
//! MQTT messages, one per value, under the component's own service topic, and corrects them if
//! externally overwritten.
//!
//! Used by `tedge-agent` (for core configuration) and each cloud mapper (for its own cloud's
//! configuration), so that the topic construction, retain flag, and reconciliation logic exist
//! only once instead of being duplicated per component.

mod actor;

#[cfg(test)]
mod tests;

use actor::ConfigPublisherActor;
use std::convert::Infallible;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LoggingSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::ServiceTopicId;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;

pub struct ConfigPublisherBuilder {
    mqtt_schema: MqttSchema,
    service_topic_id: ServiceTopicId,
    exposed_config: Vec<(String, Option<String>)>,
    box_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
    mqtt_publisher: LoggingSender<MqttMessage>,
}

impl ConfigPublisherBuilder {
    /// Creates a builder that will publish `exposed_config` — every exposable (key, value) pair
    /// in the owning component's scope, with `None` for a key that is exposable but currently
    /// unset — under `service_topic_id`'s own `config/<key>` topics.
    pub fn new(
        mqtt_schema: MqttSchema,
        service_topic_id: ServiceTopicId,
        exposed_config: Vec<(String, Option<String>)>,
        mqtt_actor: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
    ) -> Self {
        let mqtt_publisher = LoggingSender::new(
            "ConfigPublisherToMqttPublisher".into(),
            mqtt_actor.get_sender(),
        );
        let box_builder = SimpleMessageBoxBuilder::new(service_topic_id.as_str(), 16);

        let subscriptions = mqtt_schema.topics(
            EntityFilter::Entity(service_topic_id.entity()),
            ChannelFilter::Config,
        );

        let builder = Self {
            mqtt_schema,
            service_topic_id,
            exposed_config,
            box_builder,
            mqtt_publisher,
        };
        mqtt_actor.connect_sink(subscriptions, &builder);

        builder
    }
}

impl MessageSink<MqttMessage> for ConfigPublisherBuilder {
    fn get_sender(&self) -> DynSender<MqttMessage> {
        self.box_builder.get_sender()
    }
}

impl RuntimeRequestSink for ConfigPublisherBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl Builder<ConfigPublisherActor> for ConfigPublisherBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<ConfigPublisherActor, Self::Error> {
        Ok(ConfigPublisherActor::new(
            self.mqtt_schema,
            self.service_topic_id,
            self.exposed_config,
            self.box_builder.build(),
            self.mqtt_publisher,
        ))
    }
}
