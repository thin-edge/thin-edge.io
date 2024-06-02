use crate::availability::actor::TimerPayload;
pub use builder::AvailabilityBuilder;
use c8y_api::smartrest::topic::C8yTopic;
use tedge_actors::fan_in_message_type;
use tedge_api::entity_store::EntityExternalId;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::ServiceTopicId;
use tedge_api::HealthStatus;
use tedge_config::TEdgeConfig;
use tedge_config::TopicPrefix;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;

mod actor;
mod builder;
#[cfg(test)]
mod tests;

pub type TimerStart = SetTimeout<TimerPayload>;
pub type TimerComplete = Timeout<TimerPayload>;
pub type SourceHealthStatus = (ServiceTopicId, HealthStatus);

fan_in_message_type!(AvailabilityInput[EntityRegistrationMessage, SourceHealthStatus, TimerComplete] : Debug);
fan_in_message_type!(AvailabilityOutput[C8ySmartRestSetInterval117, C8yJsonInventoryUpdate] : Debug);

// TODO! Make it generic and move to c8y_api crate while refactoring c8y-mapper
#[derive(Debug)]
pub struct C8ySmartRestSetInterval117 {
    c8y_topic: C8yTopic,
    interval: i16,
}

// TODO! Make it generic and move to c8y_api crate while refactoring c8y-mapper
#[derive(Debug)]
pub struct C8yJsonInventoryUpdate {
    external_id: String,
    payload: serde_json::Value,
}

/// Required key-value pairs derived from tedge config
#[derive(Debug, Clone)]
pub struct AvailabilityConfig {
    pub main_device_id: EntityExternalId,
    pub mqtt_schema: MqttSchema,
    pub c8y_prefix: TopicPrefix,
    pub enable: bool,
    pub interval: i16,
}

impl From<&TEdgeConfig> for AvailabilityConfig {
    fn from(tedge_config: &TEdgeConfig) -> Self {
        let xid = tedge_config.device.id.try_read(tedge_config).unwrap();
        Self {
            main_device_id: xid.into(),
            mqtt_schema: MqttSchema::with_root(tedge_config.mqtt.topic_root.clone()),
            c8y_prefix: tedge_config.c8y.bridge.topic_prefix.clone(),
            enable: tedge_config.c8y.availability.enable,
            interval: tedge_config.c8y.availability.interval,
        }
    }
}
