pub mod child_device {
    use crate::smartrest::topic::C8yTopic;
    use mqtt_channel::MqttMessage;
    use tedge_config::TopicPrefix;

    pub fn new_child_device_message(child_id: &str, prefix: &TopicPrefix) -> MqttMessage {
        MqttMessage::new(
            &C8yTopic::upstream_topic(prefix),
            format!("101,{child_id},{child_id},thin-edge.io-child"),
        )
    }
}
