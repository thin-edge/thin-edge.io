use tedge_api::mqtt_topics::ChannelFilter::Command;
use tedge_api::mqtt_topics::ChannelFilter::CommandMetadata;
use tedge_api::mqtt_topics::EntityFilter::AnyEntity;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_mqtt_ext::TopicFilter;

pub fn firmware_update_topic_filter(mqtt_schema: &MqttSchema) -> TopicFilter {
    [
        mqtt_schema.topics(AnyEntity, Command(OperationType::FirmwareUpdate)),
        mqtt_schema.topics(AnyEntity, CommandMetadata(OperationType::FirmwareUpdate)),
    ]
    .into_iter()
    .collect()
}
