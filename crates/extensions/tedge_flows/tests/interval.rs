use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::CloneSender;
use tedge_actors::MappingSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_flows::ConnectedFlowRegistry;
use tedge_flows::FlowsMapperBuilder;
use tedge_mqtt_ext::DynSubscriptions;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::MqttRequest;
use tempfile::TempDir;

#[tokio::test(start_paused = true)]
async fn interval_executes_at_configured_frequency() {
    let config_dir = create_test_flow_dir();

    write_file(
        &config_dir,
        "counter.js",
        r#"
        let count = 0;
        export function onInterval(timestamp, config) {
            count++;
            return [{
                topic: "test/interval/count",
                payload: JSON.stringify({count: count})
            }];
        }
    "#,
    );

    write_file(
        &config_dir,
        "counter_flow.toml",
        r#"
        input.mqtt.topics = ["test/input"]

        [[steps]]
        script = "counter.js"
        interval = "1s"
    "#,
    );

    let captured_messages = CapturedMessages::default();
    let mut mqtt = MockMqtt::new(captured_messages.clone());
    let actor_handle = spawn_flows_actor(&config_dir, &mut mqtt).await;
    let count = || {
        captured_messages
            .retain(|msg| !msg.topic.as_ref().contains("status"))
            .count()
    };

    tick(Duration::from_millis(500)).await;
    assert_eq!(count(), 0, "Should not execute before 1s");

    tick(Duration::from_millis(600)).await;
    assert_eq!(count(), 1, "Should execute once at 1s");

    tick(Duration::from_secs(1)).await;
    assert_eq!(count(), 2, "Should execute twice at 2s");

    tick(Duration::from_secs(1)).await;
    assert_eq!(count(), 3, "Should execute three times at 3s");

    actor_handle.abort();
    let _ = actor_handle.await;
}

#[tokio::test(start_paused = true)]
async fn multiple_scripts_execute_at_independent_frequencies() {
    let config_dir = create_test_flow_dir();

    write_file(
        &config_dir,
        "fast.js",
        r#"
        let count = 0;
        export function onInterval(timestamp, config) {
            count++;
            return [{
                topic: "test/fast",
                payload: JSON.stringify({count: count})
            }];
        }
    "#,
    );

    write_file(
        &config_dir,
        "slow.js",
        r#"
        let count = 0;
        export function onInterval(timestamp, config) {
            count++;
            return [{
                topic: "test/slow",
                payload: JSON.stringify({count: count})
            }];
        }
    "#,
    );

    write_file(
        &config_dir,
        "multi_interval.toml",
        r#"
        input.mqtt.topics = ["test/input"]

        [[steps]]
        script = "fast.js"
        interval = "500ms"

        [[steps]]
        script = "slow.js"
        interval = "2s"
    "#,
    );

    let captured_messages = CapturedMessages::default();
    let mut mqtt = MockMqtt::new(captured_messages.clone());
    let actor_handle = spawn_flows_actor(&config_dir, &mut mqtt).await;
    let count = |topic: &str| {
        captured_messages
            .retain(|msg| !msg.topic.as_ref().contains("status"))
            .count_topic(topic)
    };

    tick(Duration::from_millis(500)).await;
    assert_eq!(count("test/fast"), 1, "Fast should execute once at 500ms");
    assert_eq!(count("test/slow"), 0, "Slow should not execute yet");

    tick(Duration::from_millis(500)).await;
    assert_eq!(count("test/fast"), 2, "Fast should execute twice by 1s");
    assert_eq!(count("test/slow"), 0, "Slow still shouldn't fire");

    // 1.5s -> fast should execute again, slow still not
    tick(Duration::from_millis(500)).await;

    // 2s -> both should execute
    tick(Duration::from_millis(500)).await;

    assert_eq!(count("test/fast"), 4, "Fast should execute 4 times by 2s");
    assert_eq!(count("test/slow"), 1, "Slow should execute once at 2s");

    actor_handle.abort();
    let _ = actor_handle.await;
}

#[tokio::test(start_paused = true)]
async fn script_with_oninterval_but_no_config_gets_default_1s_interval() {
    let config_dir = create_test_flow_dir();

    write_file(
        &config_dir,
        "no_interval.js",
        r#"
        export function onInterval(timestamp, config) {
            return [{
                topic: "test/default/interval",
                payload: "tick"
            }];
        }
    "#,
    );

    write_file(
        &config_dir,
        "no_interval.toml",
        r#"
        input.mqtt.topics = ["test/input"]

        [[steps]]
        script = "no_interval.js"
    "#,
    );

    let captured_messages = CapturedMessages::default();
    let mut mqtt = MockMqtt::new(captured_messages.clone());
    let actor_handle = spawn_flows_actor(&config_dir, &mut mqtt).await;
    let count = || {
        captured_messages
            .retain(|msg| !msg.topic.as_ref().contains("status"))
            .count()
    };

    tick(Duration::from_millis(500)).await;
    assert_eq!(count(), 0, "Shouldn't execute before default 1s interval");

    tick(Duration::from_millis(500)).await;
    assert_eq!(count(), 1, "Should execute once with default 1s interval");

    actor_handle.abort();
    let _ = actor_handle.await;
}

#[tokio::test(start_paused = true)]
async fn interval_executes_independently_from_message_processing() {
    let config_dir = create_test_flow_dir();

    write_file(
        &config_dir,
        "dual.js",
        r#"
        export function onMessage(msg, config) {
            return [{
                topic: "onMessage",
                payload: msg.payload
            }];
        }

        export function onInterval(timestamp, config) {
            return [{
                topic: "onInterval",
                payload: "tick"
            }];
        }
    "#,
    );

    write_file(
        &config_dir,
        "dual.toml",
        r#"
        input.mqtt.topics = ["test/input"]

        [[steps]]
        script = "dual.js"
        interval = "1s"
    "#,
    );

    let captured_messages = CapturedMessages::default();
    let mut mqtt = MockMqtt::new(captured_messages.clone());
    let actor_handle = spawn_flows_actor(&config_dir, &mut mqtt).await;
    let count = |topic: &str| {
        captured_messages
            .retain(|msg| !msg.topic.as_ref().contains("status"))
            .count_topic(topic)
    };

    tick(Duration::from_millis(1000)).await;
    assert_eq!(
        count("onInterval"),
        1,
        "Should get 1 interval message after 1s"
    );

    tick(Duration::from_millis(1000)).await;
    assert_eq!(
        count("onInterval"),
        2,
        "Should get 2 interval messages after 2s"
    );

    assert_eq!(
        count("onMessage"),
        0,
        "No input messages sent, should get 0 output messages"
    );

    // Now publish a message and verify onMessage is called but onInterval is not
    mqtt.publish("test/input", "hello").await;
    tick(Duration::from_millis(100)).await;

    assert_eq!(
        count("onMessage"),
        1,
        "Should get 1 message output after publishing input"
    );
    assert_eq!(
        count("onInterval"),
        2,
        "Interval should not have fired again"
    );

    actor_handle.abort();
    let _ = actor_handle.await;
}

#[tokio::test(start_paused = true)]
async fn very_short_intervals_execute_correctly() {
    let config_dir = create_test_flow_dir();

    write_file(
        &config_dir,
        "rapid.js",
        r#"
        let count = 0;
        export function onInterval(timestamp, config) {
            count++;
            return [{
                topic: "test/rapid",
                payload: String(count)
            }];
        }
    "#,
    );

    write_file(
        &config_dir,
        "rapid.toml",
        r#"
        input.mqtt.topics = ["test/input"]

        [[steps]]
        script = "rapid.js"
        interval = "100ms"
    "#,
    );

    let captured_messages = CapturedMessages::default();
    let mut mqtt = MockMqtt::new(captured_messages.clone());
    let actor_handle = spawn_flows_actor(&config_dir, &mut mqtt).await;
    let count = || {
        captured_messages
            .retain(|msg| !msg.topic.as_ref().contains("status"))
            .count()
    };

    tick(Duration::from_millis(100)).await;
    assert_eq!(count(), 1, "Should execute once by 100ms");

    tick(Duration::from_millis(100)).await;
    assert_eq!(count(), 2, "Should execute twice by 200ms");

    tick(Duration::from_millis(100)).await;
    assert_eq!(count(), 3, "Should execute 3 times by 300ms");

    actor_handle.abort();
    let _ = actor_handle.await;
}

#[tokio::test(start_paused = true)]
async fn interval_executes_when_time_exceeds_interval() {
    let config_dir = create_test_flow_dir();

    write_file(
        &config_dir,
        "skip.js",
        r#"
        export function onInterval(timestamp, config) {
            return [{
                topic: "test/skip",
                payload: "executed"
            }];
        }
    "#,
    );

    write_file(
        &config_dir,
        "skip.toml",
        r#"
        input.mqtt.topics = ["test/input"]

        [[steps]]
        script = "skip.js"
        interval = "120s"
    "#,
    );

    let captured_messages = CapturedMessages::default();
    let mut mqtt = MockMqtt::new(captured_messages.clone());
    let actor_handle = spawn_flows_actor(&config_dir, &mut mqtt).await;
    let count = || {
        captured_messages
            .retain(|msg| !msg.topic.as_ref().contains("status"))
            .count()
    };

    tick(Duration::from_secs(60)).await;
    assert_eq!(count(), 0, "Should not execute before 2 minutes");

    // Jump over the 2 minutes mark by waiting 2 minutes more (total 3 minutes)
    tick(Duration::from_secs(120)).await;
    assert_eq!(
        count(),
        1,
        "Should execute once even though we skipped over the 2 minute mark"
    );

    // Allow another 2 minutes to pass (total 5 minutes)
    tick(Duration::from_secs(120)).await;
    assert_eq!(count(), 2, "Should execute again after a further 2 minutes");

    actor_handle.abort();
    let _ = actor_handle.await;
}

fn create_test_flow_dir() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn write_file(dir: &TempDir, name: &str, content: &str) {
    std::fs::write(dir.path().join(name), content).expect("Failed to write file");
}

async fn tick(duration: Duration) {
    tokio::time::advance(duration).await;

    // Give actor time to process any actions triggered by the passage of time
    tokio::time::sleep(Duration::from_millis(10)).await;
}

type ActorHandle = tokio::task::JoinHandle<Result<(), tedge_actors::RuntimeError>>;

async fn spawn_flows_actor(config_dir: &TempDir, mqtt: &mut MockMqtt) -> ActorHandle {
    let flows = ConnectedFlowRegistry::new(config_dir.path().to_str().unwrap());
    let mut flows_builder = FlowsMapperBuilder::try_new(flows)
        .await
        .expect("Failed to create FlowsMapper");

    flows_builder.connect(mqtt);
    let flows_actor = flows_builder.build();

    mqtt.build();

    let handle = tokio::spawn(flows_actor.run());

    // Give actor time to initialize
    tokio::time::sleep(Duration::from_millis(10)).await;

    handle
}

struct MockMqtt {
    inbox: Option<SimpleMessageBoxBuilder<MqttRequest, MqttMessage>>,
    captured: CapturedMessages,
    sender: Mutex<Option<tedge_actors::DynSender<MqttRequest>>>,
    message_box: Option<tedge_actors::SimpleMessageBox<MqttRequest, MqttMessage>>,
}

impl MockMqtt {
    fn new(captured: CapturedMessages) -> Self {
        Self {
            inbox: Some(SimpleMessageBoxBuilder::new("MockMqtt", 16)),
            captured,
            sender: Mutex::new(None),
            message_box: None,
        }
    }

    fn build(&mut self) {
        // Build the message box after it's been connected
        let builder = self.inbox.take().unwrap();
        let message_box = builder.build();
        self.message_box = Some(message_box);
    }

    async fn publish(&mut self, topic: &str, payload: &str) {
        use tedge_actors::Sender;

        let msg = MqttMessage::new(
            &tedge_mqtt_ext::Topic::new_unchecked(topic),
            payload.as_bytes(),
        );

        if let Some(message_box) = self.message_box.as_mut() {
            message_box.send(msg).await.expect("Failed to send message");
        }
    }
}

impl MessageSource<MqttMessage, &mut DynSubscriptions> for MockMqtt {
    fn connect_sink(
        &mut self,
        config: &mut DynSubscriptions,
        peer: &impl MessageSink<MqttMessage>,
    ) {
        config.set_client_id_usize(0);
        let inbox = self
            .inbox
            .as_mut()
            .expect("Must connect sinks before building");
        inbox.connect_sink(NoConfig, peer);
    }
}

impl MessageSink<MqttRequest> for MockMqtt {
    fn get_sender(&self) -> tedge_actors::DynSender<MqttRequest> {
        let mut cached_sender = self.sender.lock().unwrap();
        if let Some(sender) = &*cached_sender {
            return sender.sender_clone();
        }

        let captured = self.captured.messages.clone();
        let inbox_sender = self.inbox.as_ref().unwrap().get_sender();
        let sender = Box::new(MappingSender::new(inbox_sender, move |req| {
            if let MqttRequest::Publish(msg) = &req {
                captured.lock().unwrap().push(msg.clone());
            }
            Some(req)
        }));

        *cached_sender = Some(sender.sender_clone());
        sender
    }
}

#[derive(Clone, Default)]
struct CapturedMessages {
    messages: Arc<Mutex<Vec<MqttMessage>>>,
}

impl CapturedMessages {
    pub fn count(&self) -> usize {
        self.messages.lock().unwrap().len()
    }

    pub fn count_topic(&self, topic: &str) -> usize {
        let msgs = self.messages.lock().unwrap();
        msgs.iter().filter(|m| m.topic.name == topic).count()
    }

    pub fn retain(&self, predicate: impl Fn(&MqttMessage) -> bool) -> &Self {
        let mut messages = self.messages.lock().unwrap();
        messages.retain(predicate);
        self
    }
}
