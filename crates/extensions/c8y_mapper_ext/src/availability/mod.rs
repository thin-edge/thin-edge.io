use crate::availability::actor::TimerPayload;
pub use builder::AvailabilityBuilder;
use c8y_api::smartrest::inventory::C8ySmartRestSetInterval117;
use std::time::Duration;
use tedge_actors::fan_in_message_type;
use tedge_api::entity_store::EntityExternalId;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::HealthStatus;
use tedge_config::ReadError;
use tedge_config::TEdgeConfig;
use tedge_config::TopicPrefix;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;

mod actor;
mod builder;
#[cfg(test)]
mod tests;

pub type TimerStart = SetTimeout<TimerPayload>;
pub type TimerComplete = Timeout<TimerPayload>;
pub type SourceHealthStatus = (EntityTopicId, HealthStatus);

fan_in_message_type!(AvailabilityInput[EntityRegistrationMessage, SourceHealthStatus, TimerComplete] : Debug);
fan_in_message_type!(AvailabilityOutput[C8ySmartRestSetInterval117, C8yJsonInventoryUpdate] : Debug);

// TODO! Make it generic and move to c8y_api crate while refactoring c8y-mapper
#[derive(Debug)]
pub struct C8yJsonInventoryUpdate {
    external_id: String,
    payload: serde_json::Value,
    pub prefix: TopicPrefix,
}

impl From<C8yJsonInventoryUpdate> for MqttMessage {
    fn from(value: C8yJsonInventoryUpdate) -> Self {
        let json_over_mqtt_topic = format!(
            "{prefix}/inventory/managedObjects/update/{external_id}",
            prefix = value.prefix,
            external_id = value.external_id
        );
        MqttMessage::new(
            &Topic::new_unchecked(&json_over_mqtt_topic),
            value.payload.to_string(),
        )
    }
}

/// Required key-value pairs derived from tedge config
#[derive(Debug, Clone)]
pub struct AvailabilityConfig {
    pub main_device_id: EntityExternalId,
    pub mqtt_schema: MqttSchema,
    pub c8y_prefix: TopicPrefix,
    pub enable: bool,
    pub interval: Duration,
}

impl AvailabilityConfig {
    pub fn try_new(
        tedge_config: &TEdgeConfig,
        c8y_profile: Option<&str>,
    ) -> Result<Self, ReadError> {
        let c8y = tedge_config.c8y.try_get(c8y_profile)?;
        let xid = c8y.device.id()?;
        Ok(Self {
            main_device_id: xid.into(),
            mqtt_schema: MqttSchema::with_root(tedge_config.mqtt.topic_root.clone()),
            c8y_prefix: c8y.bridge.topic_prefix.clone(),
            enable: c8y.availability.enable,
            interval: c8y.availability.interval.duration(),
        })
    }
}
