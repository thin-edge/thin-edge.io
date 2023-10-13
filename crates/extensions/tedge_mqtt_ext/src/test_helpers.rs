use crate::MqttMessage;
use assert_json_diff::assert_json_include;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use std::fmt::Debug;
use tedge_actors::MessageReceiver;

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
        assert_message_contains_str(&message, expected_msg);
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
        assert_message_includes_json(&message, expected_msg);
    }
}

pub fn assert_messages_contains_str<I, S>(messages: &mut Vec<MqttMessage>, expected: I)
where
    I: IntoIterator<Item = (S, S)>,
    S: AsRef<str> + Debug,
{
    let mut expected = expected.into_iter();
    loop {
        match (messages.is_empty(), expected.next()) {
            (false, Some(expected_msg)) => {
                let message = messages.remove(0);
                assert_message_contains_str(&message, expected_msg);
            }
            (true, Some(expected_msg)) => {
                panic!(
                    "Input messages exhausted while expecting: {:?}",
                    expected_msg
                )
            }
            _ => break,
        }
    }
}

pub fn assert_messages_includes_json<I, S>(messages: &mut Vec<MqttMessage>, expected: I)
where
    I: IntoIterator<Item = (S, serde_json::Value)>,
    S: AsRef<str>,
{
    let mut expected = expected.into_iter();
    loop {
        match (messages.is_empty(), expected.next()) {
            (false, Some(expected_msg)) => {
                let message = messages.remove(0);
                assert_message_includes_json(&message, expected_msg);
            }
            (true, Some(expected_msg)) => {
                panic!(
                    "Input messages exhausted while expecting message on topic: {:?} with payload: {:?}",
                    expected_msg.0.as_ref(), expected_msg.1
                )
            }
            _ => break,
        }
    }
}

pub fn assert_message_contains_str<S>(message: &Message, expected: (S, S))
where
    S: AsRef<str> + Debug,
{
    let expected_topic = expected.0.as_ref();
    let expected_payload = expected.1.as_ref();
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

pub fn assert_message_includes_json<S>(message: &Message, expected: (S, serde_json::Value))
where
    S: AsRef<str>,
{
    assert_eq!(message.topic, Topic::new_unchecked(expected.0.as_ref()));
    let payload = serde_json::from_str::<serde_json::Value>(
        message.payload_str().expect("non UTF-8 payload"),
    )
    .expect("non JSON payload");
    assert_json_include!(
        actual: payload,
        expected: expected.1
    );
}
