use crate::tests::skip_init_messages;
use crate::tests::spawn_c8y_mapper_actor;
use crate::tests::spawn_dummy_c8y_http_proxy;
use crate::tests::MockMqttBox;
use crate::tests::TestHandle;
use proptest::test_runner::Config as ProptestConfig;
use rand::seq::SliceRandom;
use rand::Rng;
use rand::SeedableRng;
use serde_json::json;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::time::Duration;
use tedge_actors::ChannelError;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeRequest;
use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::MqttRequest;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use tedge_test_utils::fs::TempTedgeDir;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);
fn proptest_config() -> ProptestConfig {
    ProptestConfig {
        // These tests are relatively slow compared to what proptest expects, don't run them too many times
        cases: 30,
        // The "properties" are all seeds; there isn't any shrinking that can be done on them
        max_shrink_iters: 0,
        ..<_>::default()
    }
}

/// A wrapper around `MockMqttBox` that simulates realistic MQTT broker
/// behavior for testing.
///
/// ## Message shuffling
/// Messages on different topics have no guaranteed relative ordering, but
/// messages within a single topic are always delivered in the order they were
/// published. Queued messages are injected one at a time, with the next topic
/// chosen randomly by the RNG.
///
/// ## Broker loopback
/// Messages published by one actor (e.g. the c8y mapper publishing an entity
/// birth) are routed back to all connected actors (e.g. the flows mapper),
/// just like a real MQTT broker would.
///
/// ## Randomized per-actor delivery order
/// When delivering a message to multiple actors (either an injected message
/// or a looped-back publish), the actors receive it in a random order with
/// a `yield_now()` between each delivery. This means an actor that receives
/// the message first may process it and publish a response *before* another
/// actor has even received the original message — creating realistic
/// interleaving without any timing hacks.
pub struct ShuffledMqttBox<'a, R> {
    inner: &'a mut MockMqttBox,
    pending: HashMap<String, VecDeque<MqttMessage>>,
    rng: &'a mut R,
    timeout: Duration,
}

impl<'a, R: Rng> ShuffledMqttBox<'a, R> {
    pub fn new(inner: &'a mut MockMqttBox, rng: &'a mut R, timeout: Duration) -> Self {
        ShuffledMqttBox {
            inner,
            pending: HashMap::new(),
            rng,
            timeout,
        }
    }

    /// Queue a message for lazy delivery. It will be sent to the mapper at a
    /// random point (respecting per-topic order) as the test reads output.
    pub fn queue(&mut self, message: MqttMessage) {
        let topic = message.topic.name.clone();
        self.pending
            .entry(topic.clone())
            .or_default()
            .push_back(message);
    }

    /// Send one pending message from a randomly-chosen topic to actors in
    /// shuffled order, yielding between each delivery. Returns `Ok(true)` if a
    /// message was sent, `Ok(false)` if there are no pending messages.
    async fn send_one(&mut self) -> Result<bool, ChannelError> {
        if self.pending.is_empty() {
            return Ok(false);
        }

        let idx = self.rng.random_range(0..self.pending.len());
        let topic = self.pending.keys().nth(idx).unwrap().to_owned();
        let mut queue = self.pending.remove(&topic).unwrap();
        let msg = queue.pop_front().unwrap();

        if !queue.is_empty() {
            self.pending.insert(topic, queue);
        }

        self.send_to_actors_shuffled(msg).await?;
        Ok(true)
    }

    /// Deliver a message to all matching actors in a random order, yielding
    /// between each delivery so that earlier recipients can process and
    /// publish before later recipients even receive the message.
    async fn send_to_actors_shuffled(&mut self, msg: MqttMessage) -> Result<(), ChannelError> {
        let mut indices: Vec<usize> = (0..self.inner.senders.len())
            .filter(|&i| self.inner.senders[i].0.accept(&msg))
            .collect();

        indices.shuffle(self.rng);

        for idx in indices {
            self.inner.senders[idx].1.send(msg.clone()).await?;
            tokio::task::yield_now().await;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl<R: Rng + Send> MessageReceiver<MqttMessage> for ShuffledMqttBox<'_, R> {
    async fn try_recv(&mut self) -> Result<Option<MqttMessage>, RuntimeRequest> {
        // Inject one pending message before reading output
        let _ = self.send_one().await;

        // Read directly from the inner receiver so we can loopback messages
        // to connected actors BEFORE applying the ignore filter.
        let deadline = tokio::time::Instant::now() + self.timeout;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let message = tokio::time::timeout(remaining, self.inner.receiver.try_recv()).await;

            let message = match message {
                Ok(msg) => msg,
                Err(_) => return Ok(None), // timeout
            };

            match message {
                Ok(Some(MqttRequest::Publish(publish))) => {
                    // Loopback: deliver to actors in shuffled order with yields
                    let _ = self.send_to_actors_shuffled(publish.clone()).await;

                    // Return to the test if not on the ignore list
                    if !self.inner.ignore_topics.accept(&publish) {
                        return Ok(Some(publish));
                    }
                    // Otherwise loop to get the next message
                }
                Ok(Some(MqttRequest::Subscribe(_))) => {
                    // Ignored
                }
                Ok(Some(MqttRequest::RetrieveRetain(sender, _))) => {
                    // No retained messages
                    sender.close_channel();
                }
                Ok(None) => return Ok(None),
                Err(e) => return Err(e),
            }
        }
    }

    async fn recv(&mut self) -> Option<MqttMessage> {
        self.try_recv().await.unwrap_or_default()
    }

    async fn recv_signal(&mut self) -> Option<RuntimeRequest> {
        self.inner.recv_signal().await
    }
}

#[proptest::property_test(config = proptest_config())]
fn birth_message_with_shuffled_entity_registration(seed: u64) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(birth_message_with_shuffled_entity_registration_impl(seed))
}

async fn birth_message_with_shuffled_entity_registration_impl(seed: u64) {
    let (mut mqtt, _keep_alive) = setup_mapper().await;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let mut mqtt = ShuffledMqttBox::new(&mut mqtt, &mut rng, TEST_TIMEOUT_MS);

    // Queue entity registrations and measurement data — they'll be injected
    // one at a time in random cross-topic order as we read mapper output.
    //
    // The measurement messages exercise the flows mapper's MessageCache:
    // if a measurement arrives before the entity is registered, the cache
    // holds it until the entity birth message triggers a flush.

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///m/temperature"),
        json!({ "temp": 42.0 }).to_string(),
    ));

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        r#"{ "@type": "child-device", "type": "RaspberryPi", "name": "Child1" }"#,
    ));

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///m/temperature"),
        json!({ "temp": 43.0 }).to_string(),
    ));

    // The mapper should produce init messages + all registrations,
    // regardless of the order the messages arrived.
    // tokio::time::sleep(Duration::from_millis(100)).await;
    skip_init_messages(&mut mqtt).await;

    // Registration responses and measurement conversions arrive in whatever
    // order the mapper processed the shuffled inputs.
    assert_received_unordered_contains_str(
        &mut mqtt,
        [
            // Entity registrations (from c8y mapper)
            (
                "c8y/s/us",
                "101,test-device:device:child1,Child1,RaspberryPi,false",
            ),
        ],
    )
    .await;
    assert_received_contains_str(
        &mut mqtt,
        [
            // Measurements (from flows mapper via MessageCache)
            ("c8y/measurement/measurements/create", "42.0"),
            ("c8y/measurement/measurements/create", "43.0"),
        ],
    )
    .await;
}

#[proptest::property_test(config = proptest_config())]
fn nested_child_registration_with_shuffled_ordering(seed: u64) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(nested_child_registration_with_shuffled_ordering_impl(seed))
}

async fn nested_child_registration_with_shuffled_ordering_impl(seed: u64) {
    let (mut mqtt, _keep_alive) = setup_mapper().await;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let mut mqtt = ShuffledMqttBox::new(&mut mqtt, &mut rng, TEST_TIMEOUT_MS);

    // Queue three nested child device births in shuffled order.
    // child1 is a direct child, child2 parents to child1, child3 parents to child2.
    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        r#"{"@type":"child-device","type":"RaspberryPi","name":"Child1"}"#,
    ));

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/child2//"),
        r#"{"@type":"child-device","@parent":"device/child1//"}"#,
    ));

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/child3//"),
        r#"{"@type":"child-device","@id":"child3","@parent":"device/child2//"}"#,
    ));

    skip_init_messages(&mut mqtt).await;

    // The mapper must register parents before children, regardless of input order.
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "101,test-device:device:child1,Child1,RaspberryPi,false",
            ),
            (
                "c8y/s/us/test-device:device:child1",
                "101,test-device:device:child2,test-device:device:child2,thin-edge.io-child,false",
            ),
            (
                "c8y/s/us/test-device:device:child2",
                "101,child3,child3,thin-edge.io-child,false",
            ),
        ],
    )
    .await;
}

#[proptest::property_test(config = proptest_config())]
fn child_service_alarm_with_shuffled_ordering(seed: u64) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(child_service_alarm_with_shuffled_ordering_impl(seed))
}

async fn child_service_alarm_with_shuffled_ordering_impl(seed: u64) {
    let (mut mqtt, _keep_alive) = setup_mapper().await;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let mut mqtt = ShuffledMqttBox::new(&mut mqtt, &mut rng, TEST_TIMEOUT_MS);

    // Queue device birth, service birth, and service alarm in shuffled order.
    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor//"),
        r#"{"@type":"child-device"}"#,
    ));

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor/service/service_child"),
        r#"{"@type":"service"}"#,
    ));

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor/service/service_child/a/custom_alarm"),
        json!({
            "severity": "critical",
            "text": "temperature alarm",
            "time": "2023-01-25T18:41:14.776170774Z",
        })
        .to_string(),
    ));

    skip_init_messages(&mut mqtt).await;

    // The mapper must output: device registration → service registration → alarm.
    assert_received_contains_str(
        &mut mqtt,
        [
            ("c8y/s/us", "101,test-device:device:external_sensor,"),
            (
                "c8y/s/us/test-device:device:external_sensor",
                "102,test-device:device:external_sensor:service:service_child,",
            ),
            (
                "c8y/s/us/test-device:device:external_sensor:service:service_child",
                "301,custom_alarm,temperature alarm,2023-01-25T18:41:14.776170774Z",
            ),
        ],
    )
    .await;
}

#[proptest::property_test(config = proptest_config())]
fn child_alarm_with_shuffled_entity_registration(seed: u64) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(child_alarm_with_shuffled_entity_registration_impl(seed))
}

async fn child_alarm_with_shuffled_entity_registration_impl(seed: u64) {
    let (mut mqtt, _keep_alive) = setup_mapper().await;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let mut mqtt = ShuffledMqttBox::new(&mut mqtt, &mut rng, TEST_TIMEOUT_MS);

    // Queue alarms interleaved with entity registration.
    // If an alarm arrives before the entity is registered, it must be cached.
    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor///a/temperature_high"),
        json!({ "severity": "minor", "text": "Temperature high" }).to_string(),
    ));

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor//"),
        r#"{"@type":"child-device"}"#,
    ));

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor///a/temperature_high"),
        json!({ "severity": "minor", "text": "Still high" }).to_string(),
    ));

    skip_init_messages(&mut mqtt).await;

    // Registration must come first, then both alarms in topic order.
    assert_received_contains_str(
        &mut mqtt,
        [
            ("c8y/s/us", "101,test-device:device:external_sensor,"),
            (
                "c8y/s/us/test-device:device:external_sensor",
                "303,temperature_high,Still high",
            ),
        ],
    )
    .await;
}

#[proptest::property_test(config = proptest_config())]
fn child_event_with_shuffled_entity_registration(seed: u64) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(child_event_with_shuffled_entity_registration_impl(seed))
}

async fn child_event_with_shuffled_entity_registration_impl(seed: u64) {
    let (mut mqtt, _keep_alive) = setup_mapper().await;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let mut mqtt = ShuffledMqttBox::new(&mut mqtt, &mut rng, TEST_TIMEOUT_MS);

    // Queue an event before the entity registration.
    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor///e/custom_event"),
        json!({
            "text": "Someone logged-in",
            "time": "2023-01-25T18:41:14.776170774Z",
        })
        .to_string(),
    ));

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor//"),
        r#"{"@type":"child-device"}"#,
    ));

    skip_init_messages(&mut mqtt).await;

    // Registration must come before the event.
    assert_received_contains_str(
        &mut mqtt,
        [
            ("c8y/s/us", "101,test-device:device:external_sensor,"),
            ("c8y/event/events/create", "custom_event"),
        ],
    )
    .await;
}

#[proptest::property_test(config = proptest_config())]
fn nested_child_alarm_with_shuffled_ordering(seed: u64) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(nested_child_alarm_with_shuffled_ordering_impl(seed))
}

async fn nested_child_alarm_with_shuffled_ordering_impl(seed: u64) {
    let (mut mqtt, _keep_alive) = setup_mapper().await;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let mut mqtt = ShuffledMqttBox::new(&mut mqtt, &mut rng, TEST_TIMEOUT_MS);

    // Queue nested child device births and an alarm from the nested child.
    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/immediate_child//"),
        json!({
            "@type": "child-device",
            "@parent": "device/main//",
            "@id": "immediate_child",
        })
        .to_string(),
    ));

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child//"),
        json!({
            "@type": "child-device",
            "@parent": "device/immediate_child//",
            "@id": "nested_child",
        })
        .to_string(),
    ));

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child///a/"),
        json!({
            "severity": "minor",
            "text": "Temperature high",
            "time": "2023-10-13T15:00:07.172674353Z",
        })
        .to_string(),
    ));

    skip_init_messages(&mut mqtt).await;

    // Parent → child → alarm, in strict dependency order.
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "101,immediate_child,immediate_child,thin-edge.io-child,false",
            ),
            (
                "c8y/s/us/immediate_child",
                "101,nested_child,nested_child,thin-edge.io-child,false",
            ),
            (
                "c8y/s/us/nested_child",
                "303,ThinEdgeAlarm,Temperature high,2023-10-13T15:00:07.172674353Z",
            ),
        ],
    )
    .await;
}

#[proptest::property_test(config = proptest_config())]
fn nested_child_service_alarm_with_shuffled_ordering(seed: u64) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(nested_child_service_alarm_with_shuffled_ordering_impl(seed))
}

async fn nested_child_service_alarm_with_shuffled_ordering_impl(seed: u64) {
    let (mut mqtt, _keep_alive) = setup_mapper().await;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let mut mqtt = ShuffledMqttBox::new(&mut mqtt, &mut rng, TEST_TIMEOUT_MS);

    // Queue the full dependency chain: parent → child → service → alarm.
    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/immediate_child//"),
        json!({
            "@type": "child-device",
            "@parent": "device/main//",
            "@id": "immediate_child",
        })
        .to_string(),
    ));

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child//"),
        json!({
            "@type": "child-device",
            "@parent": "device/immediate_child//",
            "@id": "nested_child",
        })
        .to_string(),
    ));

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child/service/nested_service"),
        json!({
            "@type": "service",
            "@parent": "device/nested_child//",
            "@id": "nested_service",
        })
        .to_string(),
    ));

    mqtt.queue(MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child/service/nested_service/a/"),
        json!({
            "severity": "minor",
            "text": "Temperature high",
            "time": "2023-10-13T15:00:07.172674353Z",
        })
        .to_string(),
    ));

    skip_init_messages(&mut mqtt).await;

    // Full dependency chain must be respected: parent → child → service → alarm.
    assert_received_contains_str(
        &mut mqtt,
        [
            ("c8y/s/us", "101,immediate_child,"),
            ("c8y/s/us/immediate_child", "101,nested_child,"),
            ("c8y/s/us/nested_child", "102,nested_service,"),
            (
                "c8y/s/us/nested_service",
                "303,ThinEdgeAlarm,Temperature high,2023-10-13T15:00:07.172674353Z",
            ),
        ],
    )
    .await;
}

/// Set up the mapper actors and return the MQTT box ready for shuffled testing.
///
/// This handles the common boilerplate: building actors, spawning them,
/// sending the bridge health message, and ignoring noisy topics.
/// The caller creates a `ShuffledMqttBox` from the returned `MockMqttBox`.
///
/// Returns `(MockMqttBox, KeepAlive)` — the second value must be kept alive
/// (bound to a `_variable`) for the duration of the test so that actor
/// channels and the temp directory are not dropped.
async fn setup_mapper() -> (MockMqttBox, Box<dyn std::any::Any>) {
    let ttd = TempTedgeDir::new();
    let TestHandle {
        mut mqtt,
        http,
        fs,
        ul,
        dl,
        avail,
    } = spawn_c8y_mapper_actor(&ttd, true).await;

    spawn_dummy_c8y_http_proxy(http);
    mqtt.ignore("te/device/main/service/tedge-mapper-c8y/status/entities");

    (mqtt, Box::new((ttd, fs, ul, dl, avail)))
}

/// Assert that the expected messages are all received, in any order.
///
/// Each expected entry is a `(topic_pattern, payload_substring)` pair.
/// For each received message, we find the first unmatched expected entry
/// whose topic matches and whose payload contains the expected substring.
/// Panics if not all expected entries are matched within the available messages.
async fn assert_received_unordered_contains_str<'a>(
    receiver: &mut (impl MessageReceiver<MqttMessage> + Send),
    expected: impl IntoIterator<Item = (&'a str, &'a str)>,
) {
    let mut remaining: Vec<(&str, &str)> = expected.into_iter().collect();
    let count = remaining.len();

    for _ in 0..count {
        let message = receiver.recv().await;
        assert!(
            message.is_some(),
            "Channel closed while still expecting {remaining:?}",
        );
        let message = message.unwrap();
        let payload = message.payload_str().expect("non UTF-8 payload");

        let matched = remaining.iter().position(|(topic, substr)| {
            TopicFilter::new_unchecked(topic).accept(&message) && payload.contains(substr)
        });

        match matched {
            Some(idx) => {
                remaining.swap_remove(idx);
            }
            None => {
                panic!(
                    "Received unexpected message: topic={}, payload={payload}\n\
                     Still expecting: {remaining:?}",
                    message.topic.name,
                );
            }
        }
    }
}
