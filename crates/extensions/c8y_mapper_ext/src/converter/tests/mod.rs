//! Tests of [`CumulocityConverter`] on its own:
//! they call the converter directly and check the messages it returns,
//! without the timing and the message routing of the mapper actor tested by [`crate::tests`].
//!
//! The tests are grouped by feature in the submodules;
//! this module holds the helpers to create a converter and register the entities it converts messages for.

use super::*;
use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::actor::IdUploadRequest;
use crate::actor::IdUploadResult;
use crate::config::BridgeConfig;
use crate::config::C8yMapperConfig;
use crate::entity_cache::InvalidExternalIdError;
use crate::mea::alarms::AlarmConverter;
use crate::mea::health::HealthStatusConverter;
use crate::mea::measurements::MeasurementConverter;
use crate::supported_operations::operation::Operation;
use crate::supported_operations::SupportedOperations;
use crate::tests::spawn_dummy_c8y_http_proxy;
use crate::Capabilities;
use anyhow::Result;
use assert_json_diff::assert_json_include;
use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
use c8y_api::proxy_url::Protocol;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::handle::C8YHttpProxy;
use serde_json::json;
use serde_json::Value;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use tedge_actors::test_helpers::FakeServerBox;
use tedge_actors::test_helpers::FakeServerBoxBuilder;
use tedge_actors::Builder;
use tedge_actors::ClientMessageBox;
use tedge_actors::CloneSender;
use tedge_actors::LoggingSender;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::http_url::FileTransferUrls;
use tedge_api::mqtt_topics::default_topic_schema;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::pending_entity_store::RegisteredEntityData;
use tedge_api::script::ShellScript;
use tedge_api::workflow::log::log_dir::OperationLogs;
use tedge_api::CommandStatus;
use tedge_api::SoftwareListCommand;
use tedge_api::SoftwareUpdateCommand;
use tedge_config::models::AutoLogUpload;
use tedge_config::models::SoftwareManagementApiFlag;
use tedge_config::models::TopicPrefix;
use tedge_config::TEdgeConfig;
use tedge_flows::ConfigError;
use tedge_flows::Message;
use tedge_flows::Transformer;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;
use tedge_mqtt_ext::test_helpers::assert_messages_matching;
use tedge_mqtt_ext::test_helpers::assert_messages_present;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::Topic;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_utils::paths::TedgePaths;
use tokio::time::Instant;

mod alarms;
mod measurements_and_events;
mod operations;
mod registration;
mod software_list;

pub(crate) fn create_c8y_converter(
    tmp_dir: &TempTedgeDir,
) -> (CumulocityConverter, FakeServerBox<HttpRequest, HttpResult>) {
    let config = c8y_converter_config(tmp_dir);
    create_c8y_converter_from_config(config)
}

pub(crate) fn c8y_converter_config(tmp_dir: &TempTedgeDir) -> C8yMapperConfig {
    tmp_dir.dir("operations").dir("c8y");
    tmp_dir.dir("tedge").dir("agent");
    tmp_dir.dir(".tedge-mapper-c8y");
    tmp_dir.dir("agent");

    let device_id = "test-device".into();
    let device_topic_id = EntityTopicId::default_main_device();
    let service_topic_id = EntityTopicId::default_main_service("tedge-mapper-c8y").unwrap();
    let tedge_config = TEdgeConfig::load_toml_str("service.ty = \"service\"");
    let c8y_host = "test.c8y.io".to_owned();
    let file_transfer_urls = FileTransferUrls::new("127.0.0.1".into(), Protocol::Http);
    let auth_proxy_addr = "127.0.0.1".into();
    let auth_proxy_port = 8001;
    let auth_proxy_protocol = Protocol::Http;
    let bridge_config = BridgeConfig {
        c8y_prefix: TopicPrefix::try_from("c8y").unwrap(),
    };

    let mut topics =
        C8yMapperConfig::default_internal_topic_filter(&"c8y".try_into().unwrap()).unwrap();
    let custom_operation_topics =
        C8yMapperConfig::get_topics_from_custom_operations(tmp_dir.path(), &bridge_config).unwrap();
    topics.add_all(custom_operation_topics);
    topics.remove_overlapping_patterns();

    let root_dir = TedgePaths::from_root_with_defaults(tmp_dir.path(), "", "");

    C8yMapperConfig::new(
        root_dir.clone().into(),
        Arc::new(OperationLogs::new(root_dir.root_dir())),
        root_dir.clone().into(),
        None,
        device_id,
        device_topic_id,
        service_topic_id,
        tedge_config.service.clone(),
        c8y_host.clone(),
        c8y_host,
        file_transfer_urls,
        topics,
        Capabilities::default(),
        auth_proxy_addr,
        auth_proxy_port,
        auth_proxy_protocol,
        MqttSchema::default(),
        bridge_config,
        false,
        SoftwareManagementApiFlag::Advanced,
        true,
        AutoLogUpload::Never,
        false,
        false,
        16184,
        "3s",
    )
}

pub(crate) fn create_c8y_converter_from_config(
    config: C8yMapperConfig,
) -> (CumulocityConverter, FakeServerBox<HttpRequest, HttpResult>) {
    let mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);
    let mqtt_publisher = LoggingSender::new("MQTT".into(), mqtt_builder.build().sender_clone());

    let mut http_builder: FakeServerBoxBuilder<HttpRequest, HttpResult> = FakeServerBox::builder();
    let http_proxy = C8YHttpProxy::new(&config, &mut http_builder);

    let mut uploader_builder: FakeServerBoxBuilder<IdUploadRequest, IdUploadResult> =
        FakeServerBox::builder();
    let uploader = ClientMessageBox::new(&mut uploader_builder);

    let mut downloader_builder: FakeServerBoxBuilder<IdDownloadRequest, IdDownloadResult> =
        FakeServerBox::builder();
    let downloader = ClientMessageBox::new(&mut downloader_builder);

    let mut http_client_builder: FakeServerBoxBuilder<HttpRequest, HttpResult> =
        FakeServerBox::builder();
    let http_client = ClientMessageBox::new(&mut http_client_builder);

    let flow_context = FlowContextHandle::default();
    let converter = CumulocityConverter::new(
        config,
        mqtt_publisher,
        http_proxy,
        uploader,
        downloader,
        http_client,
        flow_context,
    )
    .unwrap();

    (converter, http_builder.build())
}

pub(crate) async fn register_source_entities(topic: &str, converter: &mut CumulocityConverter) {
    let (topic_id, _) = converter.mqtt_schema.entity_channel_of(topic).unwrap();
    let entities = default_topic_schema::parse(&topic_id);

    for entity in entities {
        let message = entity.to_mqtt_message(&converter.mqtt_schema);
        converter
            .process_entity_metadata_message(&message)
            .await
            .unwrap();
    }
}

struct MeaConverter<T> {
    c8y_converter: CumulocityConverter,
    mea_converter: T,
    _http: FakeServerBox<HttpRequest, HttpResult>,
}

impl<T: Default + Transformer> MeaConverter<T> {
    fn new(tmp_dir: &TempTedgeDir) -> Self {
        let (c8y_converter, _http) = create_c8y_converter(tmp_dir);
        MeaConverter {
            c8y_converter,
            mea_converter: Default::default(),
            _http,
        }
    }

    fn from_c8y_config(config: C8yMapperConfig) -> Self {
        let (c8y_converter, _http) = create_c8y_converter_from_config(config);
        MeaConverter {
            c8y_converter,
            mea_converter: Default::default(),
            _http,
        }
    }

    fn set_config(&mut self, config: Value) -> Result<(), ConfigError> {
        self.mea_converter.set_config(config.into())
    }

    async fn register_source_entities(&mut self, topic: &str) {
        register_source_entities(topic, &mut self.c8y_converter).await
    }

    async fn process_entity_metadata_message(
        &mut self,
        message: &MqttMessage,
    ) -> Result<UpdateOutcome, ConversionError> {
        self.c8y_converter
            .process_entity_metadata_message(message)
            .await
    }

    async fn convert(&mut self, message: &MqttMessage) -> Vec<MqttMessage> {
        let context = self.c8y_converter.entity_cache.flow_context();
        let timestamp = SystemTime::now();
        let message: Message = message.clone().into();
        match self.mea_converter.on_message(timestamp, &message, context) {
            Ok(messages) => messages
                .into_iter()
                .filter_map(|msg| MqttMessage::try_from(msg).ok())
                .collect(),
            Err(_) => {
                vec![]
            }
        }
    }

    fn sync_messages(&mut self) -> Vec<MqttMessage> {
        let context = self.c8y_converter.entity_cache.flow_context();
        let timestamp = SystemTime::now();
        match self.mea_converter.on_interval(timestamp, context) {
            Ok(messages) => messages
                .into_iter()
                .filter_map(|msg| MqttMessage::try_from(msg).ok())
                .collect(),
            Err(_) => {
                vec![]
            }
        }
    }
}
