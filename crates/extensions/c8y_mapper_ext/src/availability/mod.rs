use crate::availability::actor::TimerPayload;
use tedge_actors::fan_in_message_type;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::TEdgeConfig;
use tedge_config::TopicPrefix;
use tedge_mqtt_ext::MqttMessage;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;

mod actor;
mod builder;
#[cfg(test)]
mod tests;

pub use builder::AvailabilityBuilder;
use tedge_api::entity_store::EntityExternalId;

pub type TimerStart = SetTimeout<TimerPayload>;
pub type TimerComplete = Timeout<TimerPayload>;

fan_in_message_type!(AvailabilityInput[MqttMessage, TimerComplete] : Debug);
fan_in_message_type!(AvailabilityOutput[MqttMessage, TimerStart] : Debug);

/// Required key-value pairs derived from tedge config
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
