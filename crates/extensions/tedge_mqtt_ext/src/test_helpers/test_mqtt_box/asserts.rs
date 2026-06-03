//! Asserts to be used with [`super::TestMqttBox`], which skip unrelated messages automatically.

use std::time::Duration;

use mqtt_channel::MqttMessage;
use tedge_actors::MessageReceiver;

use super::TestMqttBox;

pub async fn assert_received_contains_str<'a, I, M>(messages: &mut TestMqttBox<M>, expected: I)
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
    M: MessageReceiver<MqttMessage> + Send,
{
    'outer: for expected_msg in expected.into_iter() {
        // this always waits for a message even if we don't assert any more messages
        // instead we should:
        // 1. if any messages are ready immediately, add them
        // 2. search through buffered messages
        messages.recv_short().await;

        // 3. if failed to find in buffered, then wait for new messages up to timeout
        // 4. if a new message comes and matches, exit immediately
        // 5. if it comes and doesn't match, wait for more using the remaining timeout
        loop {
            if messages.contains_message(expected_msg.0, expected_msg.1) {
                continue 'outer;
            }
            let message = tokio::time::timeout(Duration::from_secs(15), messages.recv()).await;
            match message {
                Err(_) | Ok(None) => {
                    panic!(
                        "Didn't find expected message: [{}] {}\nmessage buffer: {:#?}",
                        expected_msg.0,
                        expected_msg.1,
                        messages.messages.lock().unwrap()
                    );
                }
                _ => {}
            }
        }
    }
}

pub async fn assert_received_includes_json<I, S, M>(messages: &mut TestMqttBox<M>, expected: I)
where
    I: IntoIterator<Item = (S, serde_json::Value)>,
    S: AsRef<str>,
    M: MessageReceiver<MqttMessage> + Send,
{
    'outer: for expected_msg in expected.into_iter() {
        // this always waits for a message even if we don't assert any more messages
        // instead we should:
        // 1. if any messages are ready immediately, add them
        // 2. search through buffered messages
        messages.recv_short().await;

        // 3. if failed to find in buffered, then wait for new messages up to timeout
        // 4. if a new message comes and matches, exit immediately
        // 5. if it comes and doesn't match, wait for more using the remaining timeout
        loop {
            if messages.contains_json(&expected_msg) {
                continue 'outer;
            }
            if tokio::time::timeout(Duration::from_secs(15), messages.recv())
                .await
                .is_err()
            {
                panic!(
                    "Message doesn't include json: [{}] {}\nmessage buffer: {:#?}",
                    expected_msg.0.as_ref(),
                    expected_msg.1,
                    messages.messages.lock().unwrap()
                );
            }
        }
    }
}

pub async fn assert_received_not_contains_str<'a, I, M>(messages: &mut TestMqttBox<M>, expected: I)
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
    M: MessageReceiver<MqttMessage> + Send,
{
    for expected_msg in expected.into_iter() {
        messages.recv_short().await;

        // 3. if failed to find in buffered, then wait for new messages up to timeout
        // 4. if a new message comes and matches, exit immediately
        // 5. if it comes and doesn't match, wait for more using the remaining timeout
        if !messages.none(|m| {
            m.topic.name == expected_msg.0 && m.payload_str().unwrap().contains(expected_msg.1)
        }) {
            panic!(
                "Found unexpected message: [{}] {}\nmessage buffer: {:#?}",
                expected_msg.0,
                expected_msg.1,
                messages.messages.lock().unwrap()
            );
        }
    }
}
