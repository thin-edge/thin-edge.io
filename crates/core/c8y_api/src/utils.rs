pub mod bridge {

    use mqtt_channel::Message;

    pub const C8Y_BRIDGE_HEALTH_TOPIC: &str = "tedge/health/mosquitto-c8y-bridge";
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

#[cfg(test)]
mod tests {
    use mqtt_channel::{Message, Topic};
    use test_case::test_case;

    use crate::utils::bridge::{is_c8y_bridge_up, C8Y_BRIDGE_HEALTH_TOPIC};

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
