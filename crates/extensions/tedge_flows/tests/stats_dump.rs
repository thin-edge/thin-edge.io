use camino::Utf8Path;
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tedge_actors::Actor as _;
use tedge_actors::Builder;
use tedge_actors::CloneSender as _;
use tedge_actors::DynSender;
use tedge_actors::MappingSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource as ActorMessageSource;
use tedge_actors::NoConfig;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_flows::ConnectedFlowRegistry;
use tedge_flows::FlowsMapperBuilder;
use tedge_flows::FlowsMapperConfig;
use tedge_mqtt_ext::DynSubscriptions;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::MqttRequest;
use tedge_test_utils::tracing::TracingCapture;
use tedge_utils::paths::TedgePaths;
use tempfile::TempDir;

#[tokio::test(start_paused = true)]
async fn stats_are_dumped_when_no_interval_handlers_registered() {
    let (capture, _guard) = TracingCapture::default().start();

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path();

    let js_content = r#"
        export function onMessage(message) {
            return [message];
        }
    "#;
    std::fs::write(config_dir.join("passthrough.js"), js_content).expect("Failed to write JS file");

    let config = r#"
        input.mqtt.topics = ["te/device/main///m/test"]
        steps = [{ script = "passthrough.js" }]
        output.mqtt.topic = "te/device/main///m/output"
    "#;
    std::fs::write(config_dir.join("mqtt_only_flow.toml"), config).expect("Failed to write config");

    let mut flows_builder = flows_builder(config_dir).await;
    let mut mock_mqtt = MockMqttBuilder::new();
    flows_builder.connect(&mut mock_mqtt);

    let flows_actor = flows_builder.build();
    let _mqtt_mock = mock_mqtt.build();

    let actor_handle = tokio::spawn(async move { flows_actor.run().await });

    tokio::time::advance(Duration::from_secs(300)).await;
    tokio::task::yield_now().await;

    actor_handle.abort();
    let _ = actor_handle.await;

    let log_text = capture.messages().join("\n");

    assert!(
        log_text.contains("Collect memory usage and processing statistics"),
        "Expected memory stats to be dumped after 300+ seconds. Captured logs:\n{}",
        log_text
    );
}

#[tokio::test(start_paused = true)]
async fn stats_dumped_when_interval_handlers_present() {
    let (capture, _guard) = TracingCapture::default().start();

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path();

    let js_content = r#"
        export function onMessage(message) {
            return [message];
        }

        export function onInterval() {
            return [];
        }
    "#;
    std::fs::write(config_dir.join("interval_script.js"), js_content)
        .expect("Failed to write JS file");

    let config = r#"
        input.mqtt.topics = ["te/device/main///m/test"]
        steps = [{ script = "interval_script.js", interval = "37s" }]
        output.mqtt.topic = "te/device/main///m/output"
    "#;
    std::fs::write(config_dir.join("interval_flow.toml"), config).expect("Failed to write config");

    let mut flows_builder = flows_builder(config_dir).await;
    let mut mock_mqtt = MockMqttBuilder::new();
    flows_builder.connect(&mut mock_mqtt);

    let flows_actor = flows_builder.build();
    let _mqtt_mock = mock_mqtt.build();

    let actor_handle = tokio::spawn(async move { flows_actor.run().await });

    tokio::time::advance(Duration::from_secs(300)).await;
    tokio::task::yield_now().await;

    actor_handle.abort();
    let _ = actor_handle.await;

    let log_text = capture.messages().join("\n");

    assert!(
        log_text.contains("Collect memory usage and processing statistics"),
        "Expected memory stats to be dumped after 300+ seconds. Captured logs:\n{}",
        log_text
    );
}

#[tokio::test(start_paused = true)]
async fn stats_not_dumped_before_300_seconds() {
    let (capture, _guard) = TracingCapture::default().start();

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path();

    let js_content = r#"
        export function onMessage(message) {
            return [message];
        }
    "#;
    std::fs::write(config_dir.join("passthrough.js"), js_content).expect("Failed to write JS file");

    let config = r#"
        input.mqtt.topics = ["te/device/main///m/test"]
        steps = [{ script = "passthrough.js" }]
        output.mqtt.topic = "te/device/main///m/output"
    "#;
    std::fs::write(config_dir.join("mqtt_only_flow.toml"), config).expect("Failed to write config");

    let mut flows_builder = flows_builder(config_dir).await;
    let mut mock_mqtt = MockMqttBuilder::new();
    flows_builder.connect(&mut mock_mqtt);

    let flows_actor = flows_builder.build();
    let _mqtt_mock = mock_mqtt.build();

    let actor_handle = tokio::spawn(async move { flows_actor.run().await });

    tokio::time::advance(Duration::from_secs(299)).await;
    tokio::task::yield_now().await;

    actor_handle.abort();
    let _ = actor_handle.await;

    let log_text = capture.messages().join("\n");

    assert!(
        !log_text.contains("Collect memory usage and processing statistics"),
        "Stats should not be dumped before 300 seconds. Captured logs:\n{}",
        log_text
    );
}

async fn flows_builder(config_dir: &Path) -> FlowsMapperBuilder {
    let mapper_config = HashMap::new();
    let flows_path = Utf8Path::from_path(config_dir).unwrap();
    let flows_dir = TedgePaths::from_root_with_defaults(flows_path, "", "").root_dir();
    let flows = ConnectedFlowRegistry::new(mapper_config, flows_dir).unwrap();
    let config = FlowsMapperConfig::default();
    FlowsMapperBuilder::try_new(flows, config)
        .await
        .expect("Failed to create FlowsMapperBuilder")
}

type MockMqttActor = SimpleMessageBox<MqttMessage, MqttMessage>;

struct MockMqttBuilder {
    messages: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
    sender_cache: Arc<StdMutex<Option<DynSender<MqttRequest>>>>,
    captured_messages: Arc<StdMutex<Vec<MqttMessage>>>,
}

impl MockMqttBuilder {
    fn new() -> Self {
        Self {
            messages: SimpleMessageBoxBuilder::new("MockMQTT", 32),
            sender_cache: Arc::new(StdMutex::new(None)),
            captured_messages: Arc::new(StdMutex::new(Vec::new())),
        }
    }
}

impl ActorMessageSource<MqttMessage, &mut DynSubscriptions> for MockMqttBuilder {
    fn connect_sink(
        &mut self,
        config: &mut DynSubscriptions,
        sink: &impl MessageSink<MqttMessage>,
    ) {
        config.set_client_id_usize(0);
        self.messages.connect_sink(NoConfig, sink);
    }
}

impl MessageSink<MqttRequest> for MockMqttBuilder {
    fn get_sender(&self) -> DynSender<MqttRequest> {
        let mut cached_sender = self.sender_cache.lock().unwrap();
        if let Some(sender) = &*cached_sender {
            return sender.sender_clone();
        }

        let captured_messages = self.captured_messages.clone();
        let sender = Box::new(MappingSender::new(
            self.messages.get_sender(),
            move |req: MqttRequest| match req {
                MqttRequest::Publish(msg) => {
                    captured_messages.lock().unwrap().push(msg.clone());
                    Some(msg)
                }
                MqttRequest::Subscribe(_) => None,
                MqttRequest::RetrieveRetain(_, _) => {
                    unimplemented!()
                }
            },
        ));

        *cached_sender = Some(sender.sender_clone());
        sender
    }
}

impl Builder<MockMqttActor> for MockMqttBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<MockMqttActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> MockMqttActor {
        self.messages.build()
    }
}
