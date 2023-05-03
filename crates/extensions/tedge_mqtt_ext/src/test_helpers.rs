use std::fmt::Debug;

use assert_json_diff::assert_json_include;
use mqtt_channel::Topic;
use tedge_actors::MessageReceiver;

use crate::MqttMessage;

pub async fn assert_received_contains_str<I, S>(
    messages: &mut dyn MessageReceiver<MqttMessage>,
    expected: I,
) where
    I: IntoIterator<Item = (S, S)>,
    S: AsRef<str> + Debug,
{
    for expected_msg in expected.into_iter() {
        let message = messages.recv().await;
        assert!(
            message.is_some(),
            "Channel closed while expecting: {:?}",
            expected_msg
        );
        let message = message.unwrap();
        let expected_topic = expected_msg.0.as_ref();
        let expected_payload = expected_msg.1.as_ref();
        assert_eq!(
            message.topic,
            Topic::new_unchecked(expected_topic),
            "\nReceived unexpected message: {:?}",
            message
        );
        let payload = message.payload_str().expect("non UTF-8 payload");
        assert!(
            payload.contains(expected_payload),
            "Payload assertion failed.\n Actual: {} \n Expected: {}",
            payload,
            expected_payload
        )
    }
}

pub async fn assert_received_includes_json<I, S>(
    messages: &mut dyn MessageReceiver<MqttMessage>,
    expected: I,
) where
    I: IntoIterator<Item = (S, serde_json::Value)>,
    S: AsRef<str>,
{
    for expected_msg in expected.into_iter() {
        let message = messages.recv().await.expect("MQTT channel closed");
        assert_eq!(message.topic, Topic::new_unchecked(expected_msg.0.as_ref()));
        let payload = serde_json::from_str::<serde_json::Value>(
            message.payload_str().expect("non UTF-8 payload"),
        )
        .expect("non JSON payload");
        assert_json_include!(
            actual: payload,
            expected: expected_msg.1
        );
    }
}
