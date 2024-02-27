pub mod bridge {
    use mqtt_channel::Message;

    // FIXME: doesn't account for custom topic root, use MQTT scheme API here
    pub const C8Y_BRIDGE_HEALTH_TOPIC: &str =
        "te/device/main/service/mosquitto-c8y-bridge/status/health";
    const C8Y_BRIDGE_UP_PAYLOAD: &str = "1";

    pub fn is_c8y_bridge_up(message: &Message) -> bool {
        match message.payload_str() {
            Ok(payload) => {
                message.topic.name == C8Y_BRIDGE_HEALTH_TOPIC && payload == C8Y_BRIDGE_UP_PAYLOAD
            }
            Err(_err) => false,
        }
    }
}

pub mod child_device {
    use crate::smartrest::topic::C8yTopic;
    use mqtt_channel::Message;
    use tedge_config::TopicPrefix;

    pub fn new_child_device_message(child_id: &str, prefix: &TopicPrefix) -> Message {
        Message::new(
            &C8yTopic::upstream_topic(prefix),
            format!("101,{child_id},{child_id},thin-edge.io-child"),
        )
    }
}

#[cfg(test)]
mod tests {
    use mqtt_channel::Message;
    use mqtt_channel::Topic;
    use test_case::test_case;

    use crate::utils::bridge::is_c8y_bridge_up;
    use crate::utils::bridge::C8Y_BRIDGE_HEALTH_TOPIC;

    #[test_case(C8Y_BRIDGE_HEALTH_TOPIC, "1", true)]
    #[test_case(C8Y_BRIDGE_HEALTH_TOPIC, "0", false)]
    #[test_case("tedge/not/health/topic", "1", false)]
    #[test_case("tedge/not/health/topic", "0", false)]
    fn test_bridge_is_up(topic: &str, payload: &str, expected: bool) {
        let topic = Topic::new(topic).unwrap();
        let message = Message::new(&topic, payload);

        let actual = is_c8y_bridge_up(&message);
        assert_eq!(actual, expected);
    }
}
