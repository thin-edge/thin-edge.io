use crate::MqttMessage;
use assert_json_diff::assert_json_include;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use std::fmt::Debug;
use tedge_actors::MessageReceiver;

pub async fn assert_received_contains_str<'a, I>(
    messages: &mut dyn MessageReceiver<MqttMessage>,
    expected: I,
) where
    I: IntoIterator<Item = (&'a str, &'a str)>,
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

pub fn assert_message_contains_str(message: &Message, expected: (&str, &str)) {
    let expected_topic = expected.0;
    let expected_payload = expected.1;
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

#[derive(Debug, PartialEq, Eq)]
pub enum MessagePayloadMatcher {
    StringMessage(&'static str),
    JsonMessage(serde_json::Value),
    Empty,
    Skip,
}

impl From<&'static str> for MessagePayloadMatcher {
    fn from(value: &'static str) -> Self {
        MessagePayloadMatcher::StringMessage(value)
    }
}

impl From<serde_json::Value> for MessagePayloadMatcher {
    fn from(value: serde_json::Value) -> Self {
        MessagePayloadMatcher::JsonMessage(value)
    }
}

pub fn assert_messages_matching<'a, M, I>(messages: M, expected: I)
where
    M: IntoIterator<Item = &'a Message>,
    I: IntoIterator<Item = (&'static str, MessagePayloadMatcher)>,
{
    let mut messages_iter = messages.into_iter();
    let mut expected_iter = expected.into_iter();
    loop {
        match (messages_iter.next(), expected_iter.next()) {
            (Some(message), Some(expected_msg)) => {
                let message_topic = &message.topic.name;
                let expected_topic = expected_msg.0;
                match expected_msg.1 {
                    MessagePayloadMatcher::StringMessage(str_payload) => {
                        assert_message_contains_str(message, (expected_topic, str_payload))
                    }
                    MessagePayloadMatcher::JsonMessage(json_payload) => {
                        assert_message_includes_json(message, (expected_topic, json_payload))
                    }
                    MessagePayloadMatcher::Empty => {
                        assert_eq!(
                            message_topic, expected_topic,
                            "Received message on topic: {} instead of {}",
                            message_topic, expected_topic
                        );
                        assert!(
                            message.payload_bytes().is_empty(),
                            "Received non-empty payload while expecting empty payload on {}",
                            message_topic
                        )
                    }
                    MessagePayloadMatcher::Skip => {
                        assert_eq!(
                            message_topic, expected_topic,
                            "Received message on topic: {} instead of {}",
                            message_topic, expected_topic
                        );
                        // Skipping payload validation
                    }
                }
            }
            (None, Some(expected_msg)) => {
                panic!(
                    "Input messages exhausted while expecting message on topic: {:?}",
                    expected_msg.0
                )
            }
            (Some(message), None) => {
                panic!(
                    "Additional message received than expected on topic: {:?}",
                    message.topic.name
                )
            }
            _ => return,
        }
    }
}
