pub mod child_device {
    use crate::smartrest::message_ids::CHILD_DEVICE_CREATION;
    use crate::smartrest::topic::C8yTopic;
    use mqtt_channel::MqttMessage;
    use tedge_config::TopicPrefix;

    pub fn new_child_device_message(child_id: &str, prefix: &TopicPrefix) -> MqttMessage {
        MqttMessage::new(
            &C8yTopic::upstream_topic(prefix),
            format!("{CHILD_DEVICE_CREATION},{child_id},{child_id},thin-edge.io-child"),
        )
    }
}
