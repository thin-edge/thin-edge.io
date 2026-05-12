use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Context as _;
use assert_json_diff::assert_json_matches_no_panic;
use tracing::trace;

use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::TimedMessageBox;
use tedge_actors::ChannelError;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeRequest;
use tedge_actors::Sender;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;

pub async fn assert_received_contains_str<'a, I, Box>(messages: &mut MockMqttBox<Box>, expected: I)
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
    Box: TestMqttBox,
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
            let message = tokio::time::timeout(
                messages.timeout.unwrap_or(Duration::from_secs(15)),
                messages.recv(),
            )
            .await;
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

pub async fn assert_received_includes_json<I, S, Box>(messages: &mut MockMqttBox<Box>, expected: I)
where
    I: IntoIterator<Item = (S, serde_json::Value)>,
    S: AsRef<str>,
    Box: TestMqttBox,
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
            if tokio::time::timeout(
                messages.timeout.unwrap_or(Duration::from_secs(15)),
                messages.recv(),
            )
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

pub async fn assert_received_not_contains_str<'a, I, Box>(
    messages: &mut MockMqttBox<Box>,
    expected: I,
) where
    I: IntoIterator<Item = (&'a str, &'a str)>,
    Box: TestMqttBox,
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

pub trait TestMqttBox: MessageReceiverExt<MqttMessage> + Sender<MqttMessage> + Send {}
impl<Box> TestMqttBox for Box where Box: MessageReceiverExt<MqttMessage> + Sender<MqttMessage> + Send
{}

/// An MQTT message box that stores received messages in a buffer and supports
/// assertions for stored messages.
///
/// To enable maximum flexibility in test assertions, where most tests don't
/// care about the all the messages, only ones they're directly testing, we want
/// to allow these tests to ignore these messages without losing them (in case
/// something else asserts them later). This is most easily done by gathering
/// all received messages to a buffer and then choosing to drop selected
/// messages only once they're asserted.
pub struct MockMqttBox<Box: TestMqttBox> {
    mqtt: Box,
    // TODO: immutable accessor
    pub messages: Arc<Mutex<VecDeque<MqttMessage>>>,

    // hack: some tests use different timeout and assertions need to know about it and proper
    // handling isn't done yet, so expose just duration here, will need to fix
    timeout: Option<Duration>,
}

impl<M: TestMqttBox> MockMqttBox<M> {
    pub fn new(mqtt: M) -> Self {
        Self {
            mqtt,
            messages: Default::default(),
            timeout: Default::default(),
        }
    }

    pub fn into_unbuffered(self) -> M {
        self.mqtt
    }

    pub fn with_timeout(self, duration: Duration) -> MockMqttBox<TimedMessageBox<M>> {
        // instead of putting timeout wrapper on top, put it on box
        MockMqttBox {
            mqtt: self.mqtt.with_timeout(duration),
            messages: self.messages,
            timeout: self.timeout,
        }
    }

    /// Receives messages with a short timeout.
    ///
    /// The short timeout is for emulating receiving messages which are "immediately ready". If wanted
    /// message is not immediately ready, use normal recv which waits for configured timeout.
    pub async fn recv_short(&mut self) {
        while tokio::time::timeout(Duration::from_millis(10), self.recv())
            .await
            .is_ok()
        {}
    }

    #[track_caller]
    pub fn any<P>(&mut self, predicate: P) -> bool
    where
        P: Fn(&MqttMessage) -> bool,
    {
        let mut messages = self.messages.lock().unwrap();
        let Some(pos) = messages.iter().position(predicate) else {
            return false;
        };
        messages.remove(pos);
        true
    }

    #[track_caller]
    pub fn none<P>(&mut self, predicate: P) -> bool
    where
        P: Fn(&MqttMessage) -> bool,
    {
        let messages = self.messages.lock().unwrap();
        messages.iter().position(predicate).is_none()
    }

    #[track_caller]
    pub fn contains_message(&mut self, topic: &str, payload: &str) -> bool {
        self.any(|m| m.topic.name == topic && m.payload_str().unwrap().contains(payload))
    }

    #[track_caller]
    pub fn contains_json<S>(&mut self, expected: &(S, serde_json::Value)) -> bool
    where
        S: AsRef<str>,
    {
        self.any(|m| message_includes_json(m, expected).is_ok())
    }
}

pub fn message_includes_json<S>(
    message: &MqttMessage,
    expected: &(S, serde_json::Value),
) -> anyhow::Result<()>
where
    S: AsRef<str>,
{
    anyhow::ensure!(
        TopicFilter::new_unchecked(expected.0.as_ref()).accept(message),
        "\nReceived unexpected message: {:?}",
        message
    );

    let payload = serde_json::from_str::<serde_json::Value>(
        message.payload_str().context("non UTF-8 payload")?,
    )
    .expect("non JSON payload");

    assert_json_matches_no_panic(
        &payload,
        &expected.1,
        assert_json_diff::Config::new(assert_json_diff::CompareMode::Inclusive),
    )
    .map_err(|e| anyhow::anyhow!(e))
}

#[async_trait::async_trait]
impl<M: TestMqttBox> MessageReceiver<MqttMessage> for MockMqttBox<M> {
    async fn try_recv(&mut self) -> Result<Option<MqttMessage>, RuntimeRequest> {
        trace!("calling MockMqttBox::try_recv");
        let message = self.mqtt.try_recv().await?;
        trace!("message returned");
        if let Some(message) = &message {
            self.messages.lock().unwrap().push_back(message.clone());
        }
        Ok(message)
    }

    async fn recv(&mut self) -> Option<MqttMessage> {
        self.try_recv().await.unwrap_or_default()
    }

    async fn recv_signal(&mut self) -> Option<RuntimeRequest> {
        self.mqtt.recv_signal().await
    }
}

#[async_trait::async_trait]
impl<M: TestMqttBox> Sender<MqttMessage> for MockMqttBox<M> {
    async fn send(&mut self, message: MqttMessage) -> Result<(), ChannelError> {
        self.mqtt.send(message).await
    }
}
