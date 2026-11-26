//! Tests of the whole Cumulocity mapper actor:
//! they publish MQTT messages to a spawned mapper and check the messages it publishes,
//! with fake HTTP, upload and download servers.
//! [`crate::converter::tests`] tests the converter on its own.
//!
//! The tests are grouped by feature in the submodules;
//! this module holds the helpers to spawn the mapper and send it messages and operations.

use super::actor::C8yMapperBuilder;
use super::config::C8yMapperConfig;
use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::actor::IdUploadRequest;
use crate::actor::IdUploadResult;
use crate::availability::AvailabilityBuilder;
use crate::config::BridgeConfig;
use crate::operations::OperationHandler;
use crate::Capabilities;
use c8y_api::json_c8y::C8yEventResponse;
use c8y_api::json_c8y::InternalIdResponse;
use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
use c8y_api::proxy_url::Protocol;
use camino::Utf8Path;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::test_helpers::FakeServerBox;
use tedge_actors::test_helpers::FakeServerBoxBuilder;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::TimedMessageBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::LoggingReceiver;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::NoMessage;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::http_url::FileTransferUrls;
use tedge_api::mqtt_topics::ChannelFilter::AnySignal;
use tedge_api::mqtt_topics::EntityFilter::AnyEntity;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::workflow::log::log_dir::OperationLogs;
use tedge_config::models::AutoLogUpload;
use tedge_config::models::SoftwareManagementApiFlag;
use tedge_config::models::TopicPrefix;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::tedge_toml::C8Y_MQTT_PAYLOAD_LIMIT;
use tedge_config::TEdgeConfig;
use tedge_downloader_ext::DownloadResponse;
use tedge_file_system_ext::FsWatchEvent;
use tedge_flows::ConnectedFlowRegistry;
use tedge_flows::FlowsMapperBuilder;
use tedge_flows::FlowsMapperConfig;
use tedge_http_ext::test_helpers::HttpResponseBuilder;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;
use tedge_mqtt_ext::test_helpers::TestMqttBox;
use tedge_mqtt_ext::DynSubscriptions;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::MqttRequest;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use tedge_test_utils::fs::with_exec_permission;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_utils::paths::TedgePaths;

use tedge_mqtt_ext::test_helpers::test_mqtt_box::assert_received_contains_str;
use tedge_mqtt_ext::test_helpers::test_mqtt_box::assert_received_includes_json;
use tedge_mqtt_ext::test_helpers::test_mqtt_box::assert_received_not_contains_str;

mod alarms_and_events;
mod custom_operations;
mod operations;
mod registration;
mod supported_operations;
mod workflow_operations;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

fn create_thin_edge_operations(ttd: &TempTedgeDir, ops: Vec<&str>) {
    let p1 = ttd.dir("operations");
    let tedge_ops_dir = p1.dir("c8y");
    for op in ops {
        tedge_ops_dir.file(op);
    }
}

fn create_thin_edge_child_operations(ttd: &TempTedgeDir, child_id: &str, ops: Vec<&str>) {
    let p1 = ttd.dir("operations");
    let tedge_ops_dir = p1.dir("c8y");
    let child_ops_dir = tedge_ops_dir.dir(child_id);
    for op in ops {
        child_ops_dir.file(op);
    }
}

/// A mapper spawned with the default test configuration, with the directory it uses
///
/// The message boxes of the mapper, other than MQTT, are kept open until the end of the test,
/// as the mapper stops processing some messages when one of them is closed.
pub(crate) struct TestMapper {
    dir: TempTedgeDir,
    _boxes: Box<dyn std::any::Any>,
}

impl std::ops::Deref for TestMapper {
    type Target = TempTedgeDir;

    fn deref(&self) -> &TempTedgeDir {
        &self.dir
    }
}

/// Spawns a mapper with the default test configuration, returning its MQTT connection
async fn spawn_mapper() -> (
    TestMapper,
    TestMqttBox<TimedMessageBox<MockMqttBoxUnbuffered>>,
) {
    let dir = TempTedgeDir::new();
    let TestHandle {
        mqtt,
        http,
        fs,
        ul,
        dl,
        avail,
    } = spawn_c8y_mapper_actor(&dir, true).await;
    let mapper = TestMapper {
        dir,
        _boxes: Box::new((http, fs, ul, dl, avail)),
    };
    (mapper, mqtt.with_timeout(TEST_TIMEOUT_MS))
}

/// Spawns a mapper with the given configuration and directory, returning its MQTT connection
///
/// The HTTP requests of the mapper are answered by a dummy Cumulocity proxy.
async fn spawn_mapper_with_config(
    dir: TempTedgeDir,
    config: C8yMapperConfig,
) -> (
    TestMapper,
    TestMqttBox<TimedMessageBox<MockMqttBoxUnbuffered>>,
) {
    let TestHandle {
        mqtt,
        http,
        fs,
        ul,
        dl,
        avail,
    } = spawn_c8y_mapper_actor_with_config(&dir, config, true).await;
    spawn_dummy_c8y_http_proxy(http);
    let mapper = TestMapper {
        dir,
        _boxes: Box::new((fs, ul, dl, avail)),
    };
    (mapper, mqtt.with_timeout(TEST_TIMEOUT_MS))
}

/// Publishes a message to the mapper
async fn publish<M: Sender<MqttMessage>>(
    mqtt: &mut TestMqttBox<M>,
    topic: &str,
    payload: impl Into<Vec<u8>>,
) {
    mqtt.send(MqttMessage::new(&Topic::new_unchecked(topic), payload))
        .await
        .expect("Send failed");
}

pub(crate) async fn spawn_c8y_mapper_actor(tmp_dir: &TempTedgeDir, init: bool) -> TestHandle {
    spawn_c8y_mapper_actor_with_config(tmp_dir, test_mapper_config(tmp_dir), init).await
}

pub(crate) struct TestHandle {
    pub mqtt: TestMqttBox<MockMqttBoxUnbuffered>,
    pub http: FakeServerBox<HttpRequest, HttpResult>,
    pub fs: SimpleMessageBox<NoMessage, FsWatchEvent>,
    pub ul: FakeServerBox<IdUploadRequest, IdUploadResult>,
    pub dl: FakeServerBox<IdDownloadRequest, IdDownloadResult>,
    pub avail: SimpleMessageBox<MqttMessage, MqttMessage>,
}

pub(crate) async fn spawn_c8y_mapper_actor_with_config(
    tmp_dir: &TempTedgeDir,
    config: C8yMapperConfig,
    init: bool,
) -> TestHandle {
    let builders = c8y_mapper_builder(tmp_dir, config, init).await;

    let actor = builders.c8y.build();
    tokio::spawn(async move { actor.run().await });

    let actor = builders.flows.build();
    tokio::spawn(async move { actor.run().await });

    TestHandle {
        mqtt: builders.mqtt.build(),
        http: builders.http.build(),
        fs: builders.fs.build(),
        ul: builders.ul.build(),
        dl: builders.dl.build(),
        avail: builders.avail.build(),
    }
}

pub(crate) struct TestHandleBuilder {
    pub c8y: C8yMapperBuilder,
    pub flows: FlowsMapperBuilder,
    pub mqtt: MockMqttBoxBuilder,
    pub http: FakeServerBoxBuilder<HttpRequest, HttpResult>,
    pub fs: SimpleMessageBoxBuilder<NoMessage, FsWatchEvent>,
    pub ul: FakeServerBoxBuilder<IdUploadRequest, IdUploadResult>,
    pub dl: FakeServerBoxBuilder<IdDownloadRequest, IdDownloadResult>,
    pub avail: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
}

pub(crate) async fn c8y_mapper_builder(
    tmp_dir: &TempTedgeDir,
    config: C8yMapperConfig,
    init: bool,
) -> TestHandleBuilder {
    if init {
        tmp_dir.dir("operations").dir("c8y");
        tmp_dir.dir("mappers").dir("c8y");
        tmp_dir.dir(".tedge-mapper-c8y");
    }

    let mut mqtt_builder = MockMqttBoxBuilder::new();
    let mut http_builder: FakeServerBoxBuilder<HttpRequest, HttpResult> =
        FakeServerBoxBuilder::default();
    let mut fs_watcher_builder: SimpleMessageBoxBuilder<NoMessage, FsWatchEvent> =
        SimpleMessageBoxBuilder::new("FS", 5);
    let mut uploader_builder: FakeServerBoxBuilder<IdUploadRequest, IdUploadResult> =
        FakeServerBoxBuilder::default();
    let mut downloader_builder: FakeServerBoxBuilder<IdDownloadRequest, IdDownloadResult> =
        FakeServerBoxBuilder::default();
    C8yMapperBuilder::init(&config).await.unwrap();
    let mut c8y_mapper_builder = C8yMapperBuilder::try_new(
        config,
        &mut mqtt_builder,
        &mut http_builder,
        &mut uploader_builder,
        &mut downloader_builder,
        &mut fs_watcher_builder,
    )
    .unwrap();

    let mut availability_box_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("Availability", 10);
    availability_box_builder
        .connect_source(AvailabilityBuilder::channels(), &mut c8y_mapper_builder);
    c8y_mapper_builder.connect_source(NoConfig, &mut availability_box_builder);

    let mapper_dir = tmp_dir.path().join("mappers").join("c8y");
    let managed_dir = TedgePaths::from_root_with_defaults(&mapper_dir, "", "").root_dir();
    let flows_dir = tedge_flows::managed_flows_dir(&managed_dir);
    flows_dir.ensure().await.unwrap();
    let mapper_config = HashMap::new();
    let mut flows = ConnectedFlowRegistry::new(mapper_config, flows_dir).unwrap();
    crate::load_builtin_transformers(&mut flows);
    c8y_mapper_builder
        .persist_builtin_flows(&mut flows)
        .await
        .unwrap();
    let service_config = FlowsMapperConfig::new(
        "te/device/main/service/tedge-mapper-c8y",
        Duration::from_secs(300),
        false,
        false,
        false,
    );

    let mut flows_mapper = FlowsMapperBuilder::try_new(flows, service_config)
        .await
        .unwrap();
    flows_mapper.connect(&mut mqtt_builder);
    // The fs_watcher_builder is a fake actor which can only handle at most one peer
    // => for these tests, the flows mapper is only connected to mqtt ignoring all fs events
    c8y_mapper_builder.set_flow_context(flows_mapper.context_handle());

    TestHandleBuilder {
        c8y: c8y_mapper_builder,
        flows: flows_mapper,
        mqtt: mqtt_builder,
        http: http_builder,
        fs: fs_watcher_builder,
        ul: uploader_builder,
        dl: downloader_builder,
        avail: availability_box_builder,
    }
}

pub(crate) fn test_mapper_config(tmp_dir: &TempTedgeDir) -> C8yMapperConfig {
    let device_name = "test-device".into();
    let device_topic_id = EntityTopicId::default_main_device();
    let service_topic_id = EntityTopicId::default_main_service("tedge-mapper-c8y").unwrap();
    let config = TEdgeConfig::load_toml_str("service.ty = \"service\"");
    let c8y_host = "test.c8y.io".to_owned();
    let file_transfer_urls = FileTransferUrls::new("localhost:8888".into(), Protocol::Http);
    let mqtt_schema = MqttSchema::default();
    let auth_proxy_addr = "127.0.0.1".into();
    let auth_proxy_port = 8001;
    let bridge_config = BridgeConfig {
        c8y_prefix: TopicPrefix::try_from("c8y").unwrap(),
    };

    let mut topics =
        C8yMapperConfig::default_internal_topic_filter(&"c8y".try_into().unwrap()).unwrap();
    let custom_operation_topics =
        C8yMapperConfig::get_topics_from_custom_operations(tmp_dir.path(), &bridge_config).unwrap();
    topics.add_all(custom_operation_topics);
    topics.add_unchecked("te/+/+/+/+/cmd/software_update");
    topics.add_unchecked("te/+/+/+/+/cmd/software_update/+");
    topics.add_unchecked("te/+/+/+/+/cmd/+");
    topics.add_unchecked("te/+/+/+/+/cmd/+/+");

    let capabilities = Capabilities::default();

    let operation_topics = OperationHandler::topic_filter(&capabilities)
        .into_iter()
        .map(|(e, c)| mqtt_schema.topics(e, c))
        .collect();
    topics.add_all(operation_topics);

    topics.add_all(C8yMapperConfig::default_external_topic_filter());
    topics.add_all(mqtt_schema.topics(AnyEntity, AnySignal));

    topics.remove_overlapping_patterns();

    let root_dir = TedgePaths::from_root_with_defaults(tmp_dir.path(), "", "");
    let _agent_log_dir = tmp_dir.dir("agent");

    C8yMapperConfig::new(
        root_dir.clone().into(),
        Arc::new(OperationLogs::new(root_dir.root_dir())),
        root_dir.clone().into(),
        None,
        device_name,
        device_topic_id,
        service_topic_id,
        config.service.clone(),
        c8y_host.clone(),
        c8y_host,
        file_transfer_urls,
        topics,
        capabilities,
        auth_proxy_addr,
        auth_proxy_port,
        Protocol::Http,
        MqttSchema::default(),
        bridge_config,
        false,
        SoftwareManagementApiFlag::Advanced,
        true,
        AutoLogUpload::Never,
        false,
        false,
        C8Y_MQTT_PAYLOAD_LIMIT,
        "100ms",
    )
}

pub(crate) fn spawn_dummy_c8y_http_proxy(mut http: FakeServerBox<HttpRequest, HttpResult>) {
    tokio::spawn(async move {
        while let Some(request) = http.recv().await {
            let uri = request.uri().path();
            eprintln!("C8Y Proxy: {} {uri}", request.method());
            if uri.starts_with("/c8y/inventory/managedObjects/") {
                let _ = http
                    .send(HttpResponseBuilder::new().status(200).build())
                    .await;
            } else if uri == "/c8y/event/events/" {
                let response = C8yEventResponse {
                    id: "dummy-event-id-1234".to_string(),
                };
                let _ = http
                    .send(
                        HttpResponseBuilder::new()
                            .status(200)
                            .json(&response)
                            .build(),
                    )
                    .await;
            } else if let Some(id) = uri.strip_prefix("/c8y/identity/externalIds/c8y_Serial/") {
                let response = InternalIdResponse::new(id, id);
                let _ = http
                    .send(
                        HttpResponseBuilder::new()
                            .status(200)
                            .json(&response)
                            .build(),
                    )
                    .await;
            } else if uri.strip_prefix("/te/v1/files").is_some() {
                let _ = http
                    .send(HttpResponseBuilder::new().status(200).build())
                    .await;
            }
        }
    });
}

pub(crate) struct MockMqttBoxBuilder {
    input_sender: mpsc::Sender<MqttRequest>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
    output_sender: Vec<(TopicFilter, DynSender<MqttMessage>)>,
    input_receiver: LoggingReceiver<MqttRequest>,
}

impl MockMqttBoxBuilder {
    pub fn new() -> Self {
        let (input_sender, input_receiver) = mpsc::channel(16);
        let (signal_sender, signal_receiver) = mpsc::channel(4);
        let output_sender = Vec::new();
        let input_receiver =
            LoggingReceiver::new("MockMQTT".to_string(), input_receiver, signal_receiver);

        MockMqttBoxBuilder {
            input_sender,
            signal_sender,
            output_sender,
            input_receiver,
        }
    }

    pub fn build(self) -> TestMqttBox<MockMqttBoxUnbuffered> {
        let mut ignore_topics = TopicFilter::empty();
        ignore_topics.add_unchecked("te/+/+/+/+/status/flows");

        TestMqttBox::new(MockMqttBoxUnbuffered {
            ignore_topics,
            receiver: self.input_receiver,
            senders: self.output_sender,
        })
    }
}

impl MessageSource<MqttMessage, TopicFilter> for MockMqttBoxBuilder {
    fn connect_sink(&mut self, config: TopicFilter, peer: &impl MessageSink<MqttMessage>) {
        self.output_sender.push((config, peer.get_sender()));
    }
}

impl MessageSink<MqttMessage> for MockMqttBoxBuilder {
    fn get_sender(&self) -> DynSender<MqttMessage> {
        self.input_sender.sender_clone()
    }
}

impl MessageSource<MqttMessage, &mut DynSubscriptions> for MockMqttBoxBuilder {
    fn connect_sink(
        &mut self,
        config: &mut DynSubscriptions,
        peer: &impl MessageSink<MqttMessage>,
    ) {
        let client_id = self.output_sender.len();
        config.set_client_id_usize(client_id);
        self.output_sender
            .push((TopicFilter::new_unchecked("#"), peer.get_sender()));
    }
}

impl MessageSink<MqttRequest> for MockMqttBoxBuilder {
    fn get_sender(&self) -> DynSender<MqttRequest> {
        self.input_sender.sender_clone()
    }
}

impl RuntimeRequestSink for MockMqttBoxBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.signal_sender.sender_clone()
    }
}

pub struct MockMqttBoxUnbuffered {
    pub(crate) ignore_topics: TopicFilter,
    pub(crate) receiver: LoggingReceiver<MqttRequest>,
    pub(crate) senders: Vec<(TopicFilter, DynSender<MqttMessage>)>,
}

#[async_trait::async_trait]
impl MessageReceiver<MqttMessage> for MockMqttBoxUnbuffered {
    async fn try_recv(&mut self) -> Result<Option<MqttMessage>, RuntimeRequest> {
        loop {
            let message = self.receiver.try_recv().await;
            match message {
                Ok(Some(MqttRequest::Publish(publish))) => {
                    // Forward to all matching subscribers (simulating MQTT broker loopback)
                    for (topic, sender) in self.senders.iter_mut() {
                        if topic.accept(&publish) {
                            let _ = sender.send(publish.clone()).await;
                        }
                    }
                    if !self.ignore_topics.accept(&publish) {
                        return Ok(Some(publish));
                    }
                }
                Ok(None) => return Ok(None),
                Err(e) => return Err(e),

                Ok(Some(MqttRequest::Subscribe(_))) => {
                    // Ignored
                }
                Ok(Some(MqttRequest::RetrieveRetain(sender, _))) => {
                    // No retained messages
                    sender.close_channel();
                }
            }
        }
    }

    async fn recv(&mut self) -> Option<MqttMessage> {
        self.try_recv().await.unwrap_or_default()
    }

    async fn recv_signal(&mut self) -> Option<RuntimeRequest> {
        self.receiver.recv_signal().await
    }
}

#[async_trait::async_trait]
impl Sender<MqttMessage> for MockMqttBoxUnbuffered {
    async fn send(&mut self, message: MqttMessage) -> Result<(), ChannelError> {
        for (topic, sender) in self.senders.iter_mut() {
            if topic.accept(&message) {
                sender.send(message.clone()).await?;
            }
        }
        Ok(())
    }
}
