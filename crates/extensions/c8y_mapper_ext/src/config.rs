use crate::supported_operations::C8yPrefix;
use crate::supported_operations::Operations;
use crate::supported_operations::OperationsError;
use crate::Capabilities;
use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
use c8y_api::proxy_url::Protocol;
use c8y_api::proxy_url::ProxyUrlGenerator;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::C8YHttpConfig;
use camino::Utf8Path;
use serde_json::Value;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use tedge_api::mqtt_topics::ChannelFilter::AnyCommand;
use tedge_api::mqtt_topics::ChannelFilter::AnyCommandMetadata;
use tedge_api::mqtt_topics::EntityFilter::AnyEntity;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::IdGenerator;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::TopicIdError;
use tedge_api::path::DataDir;
use tedge_api::service_health_topic;
use tedge_api::substitution::Record;
use tedge_config::models::AutoLogUpload;
use tedge_config::models::SoftwareManagementApiFlag;
use tedge_config::models::TopicPrefix;
use tedge_config::tedge_toml::ConfigNotSet;
use tedge_config::tedge_toml::MultiError;
use tedge_config::tedge_toml::ReadError;
use tedge_config::tedge_toml::TEdgeConfigReaderService;
use tedge_config::TEdgeConfig;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use tracing::log::warn;

const STATE_DIR_NAME: &str = ".tedge-mapper-c8y";
const C8Y_CLOUD: &str = "c8y";
const SUPPORTED_OPERATIONS_DIRECTORY: &str = "operations";

pub struct C8yMapperConfig {
    pub device_id: String,
    pub device_topic_id: EntityTopicId,
    pub device_type: String,
    pub service: TEdgeConfigReaderService,
    pub c8y_host: String,
    pub c8y_mqtt: String,
    pub tedge_http_host: Arc<str>,
    pub topics: TopicFilter,
    pub capabilities: Capabilities,
    pub auth_proxy_addr: Arc<str>,
    pub auth_proxy_port: u16,
    pub auth_proxy_protocol: Protocol,
    pub mqtt_schema: MqttSchema,
    pub enable_auto_register: bool,
    pub clean_start: bool,
    pub bridge_config: BridgeConfig,
    pub bridge_in_mapper: bool,
    pub software_management_api: SoftwareManagementApiFlag,
    pub software_management_with_types: bool,
    pub auto_log_upload: AutoLogUpload,
    pub bridge_service_name: String,
    pub bridge_health_topic: Topic,
    pub smartrest_use_operation_id: bool,
    pub smartrest_child_device_create_with_device_marker: bool,

    pub data_dir: DataDir,
    pub config_dir: Arc<Utf8Path>,
    pub logs_path: Arc<Utf8Path>,
    pub ops_dir: Arc<Utf8Path>,
    pub state_dir: Arc<Utf8Path>,
    pub tmp_dir: Arc<Utf8Path>,

    pub max_mqtt_payload_size: u32,
}

impl C8yMapperConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config_dir: Arc<Utf8Path>,
        logs_path: Arc<Utf8Path>,
        data_dir: DataDir,
        tmp_dir: Arc<Utf8Path>,

        device_id: String,
        device_topic_id: EntityTopicId,
        device_type: String,
        service: TEdgeConfigReaderService,
        c8y_host: String,
        c8y_mqtt: String,
        tedge_http_host: Arc<str>,
        topics: TopicFilter,
        capabilities: Capabilities,
        auth_proxy_addr: Arc<str>,
        auth_proxy_port: u16,
        auth_proxy_protocol: Protocol,
        mqtt_schema: MqttSchema,
        enable_auto_register: bool,
        clean_start: bool,
        bridge_config: BridgeConfig,
        bridge_in_mapper: bool,
        software_management_api: SoftwareManagementApiFlag,
        software_management_with_types: bool,
        auto_log_upload: AutoLogUpload,
        smartrest_use_operation_id: bool,
        smartrest_child_device_create_with_device_marker: bool,
        max_mqtt_payload_size: u32,
    ) -> Self {
        let ops_dir = config_dir
            .join(SUPPORTED_OPERATIONS_DIRECTORY)
            .join(C8Y_CLOUD)
            .into();
        let state_dir = config_dir.join(STATE_DIR_NAME).into();

        let bridge_service_name = if bridge_in_mapper {
            format!("tedge-mapper-bridge-{}", bridge_config.c8y_prefix)
        } else {
            format!("mosquitto-{}-bridge", bridge_config.c8y_prefix)
        };
        let bridge_health_topic =
            service_health_topic(&mqtt_schema, &device_topic_id, &bridge_service_name);

        Self {
            data_dir,
            device_id,
            device_topic_id,
            device_type,
            service,
            c8y_host,
            c8y_mqtt,
            tedge_http_host,
            topics,
            capabilities,
            auth_proxy_addr,
            auth_proxy_port,
            auth_proxy_protocol,
            mqtt_schema,
            enable_auto_register,
            clean_start,
            bridge_config,
            bridge_in_mapper,
            software_management_api,
            software_management_with_types,
            auto_log_upload,
            bridge_service_name,
            bridge_health_topic,
            smartrest_use_operation_id,
            smartrest_child_device_create_with_device_marker,

            config_dir,
            logs_path,
            ops_dir,
            state_dir,
            tmp_dir,

            max_mqtt_payload_size,
        }
    }

    pub fn from_tedge_config(
        config_dir: impl AsRef<Utf8Path>,
        tedge_config: &TEdgeConfig,
        c8y_profile: Option<&str>,
    ) -> Result<C8yMapperConfig, C8yMapperConfigBuildError> {
        let config_dir: Arc<Utf8Path> = config_dir.as_ref().into();

        let logs_path = tedge_config.logs.path.as_path().into();
        let data_dir: DataDir = tedge_config.data.path.as_path().to_owned().into();
        let tmp_dir = tedge_config.tmp.path.as_path().into();

        let c8y_config = tedge_config.c8y.try_get(c8y_profile)?;
        let device_id = c8y_config.device.id()?.to_string();
        let device_type = tedge_config.device.ty.clone();
        let device_topic_id = EntityTopicId::from_str(&tedge_config.mqtt.device_topic_id)?;
        let service = tedge_config.service.clone();
        let c8y_host = c8y_config.http.or_config_not_set()?.to_string();
        let c8y_mqtt = c8y_config.mqtt.or_config_not_set()?.to_string();
        let tedge_http_address = tedge_config.http.client.host.clone();
        let tedge_http_port = tedge_config.http.client.port;
        let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
        let auth_proxy_addr = c8y_config.proxy.client.host.clone();
        let auth_proxy_port = c8y_config.proxy.client.port;
        let auth_proxy_protocol = c8y_config
            .proxy
            .cert_path
            .or_none()
            .map_or(Protocol::Http, |_| Protocol::Https);

        let tedge_http_host = format!("{}:{}", tedge_http_address, tedge_http_port).into();

        let capabilities = Capabilities {
            log_upload: c8y_config.enable.log_upload,
            config_snapshot: c8y_config.enable.config_snapshot,
            config_update: c8y_config.enable.config_update,
            firmware_update: c8y_config.enable.firmware_update,
            device_profile: c8y_config.enable.device_profile,
        };
        let bridge_config = BridgeConfig {
            c8y_prefix: c8y_config.bridge.topic_prefix.clone(),
        };

        let mut topics = Self::default_internal_topic_filter(&bridge_config.c8y_prefix)?;

        let enable_auto_register = c8y_config.entity_store.auto_register;
        let clean_start = c8y_config.entity_store.clean_start;

        let software_management_api = c8y_config.software_management.api;
        let software_management_with_types = c8y_config.software_management.with_types;

        let auto_log_upload = c8y_config.operations.auto_log_upload;
        let smartrest_use_operation_id = c8y_config.smartrest.use_operation_id;
        let smartrest_child_device_create_with_device_marker =
            c8y_config.smartrest.child_device.create_with_device_marker;
        let max_mqtt_payload_size = c8y_config.mapper.mqtt.max_payload_size.0;

        // Add command topics
        topics.add_all(mqtt_schema.topics(AnyEntity, AnyCommand));
        topics.add_all(mqtt_schema.topics(AnyEntity, AnyCommandMetadata));

        // Add user configurable external topic filters
        for topic in c8y_config.topics.0.clone() {
            if topics.add(&topic).is_err() {
                warn!("The configured topic '{topic}' is invalid and ignored.");
            }
        }

        // Add custom operation topics
        let custom_operation_topics =
            Self::get_topics_from_custom_operations(config_dir.as_std_path(), &bridge_config)?;

        topics.add_all(custom_operation_topics);

        topics.remove_overlapping_patterns();

        let bridge_in_mapper = tedge_config.mqtt.bridge.built_in;

        Ok(C8yMapperConfig::new(
            config_dir,
            logs_path,
            data_dir,
            tmp_dir,
            device_id,
            device_topic_id,
            device_type,
            service,
            c8y_host,
            c8y_mqtt,
            tedge_http_host,
            topics,
            capabilities,
            auth_proxy_addr,
            auth_proxy_port,
            auth_proxy_protocol,
            mqtt_schema,
            enable_auto_register,
            clean_start,
            bridge_config,
            bridge_in_mapper,
            software_management_api,
            software_management_with_types,
            auto_log_upload,
            smartrest_use_operation_id,
            smartrest_child_device_create_with_device_marker,
            max_mqtt_payload_size,
        ))
    }

    pub fn default_internal_topic_filter(
        prefix: &TopicPrefix,
    ) -> Result<TopicFilter, C8yMapperConfigError> {
        let topic_filter: TopicFilter = vec![
            "c8y-internal/alarms/+/+/+/+/+/a/+",
            C8yTopic::SmartRestRequest.with_prefix(prefix).as_str(),
            &C8yDeviceControlTopic::name(prefix),
        ]
        .try_into()
        .expect("topics that mapper should subscribe to");

        Ok(topic_filter)
    }

    pub fn get_topics_from_custom_operations(
        config_dir: &Path,
        bridge_config: &BridgeConfig,
    ) -> Result<TopicFilter, C8yMapperConfigError> {
        let mut topic_filter = TopicFilter::empty();

        if let Ok(operations) = Operations::try_new(
            config_dir
                .join(SUPPORTED_OPERATIONS_DIRECTORY)
                .join(C8Y_CLOUD),
            bridge_config,
        ) {
            for topic in operations.topics_for_operations() {
                topic_filter.add(&topic)?;
            }
        }

        Ok(topic_filter)
    }

    /// List of all possible external topics that Cumulocity mapper addresses. For testing purpose.
    #[cfg(test)]
    pub fn default_external_topic_filter() -> TopicFilter {
        vec![
            "te/+/+/+/+",
            "te/+/+/+/+/twin/+",
            "te/+/+/+/+/m/+",
            "te/+/+/+/+/e/+",
            "te/+/+/+/+/a/+",
            "te/+/+/+/+/status/health",
        ]
        .try_into()
        .unwrap()
    }

    pub fn id_generator(&self) -> IdGenerator {
        IdGenerator::new(&format!("{}-mapper", self.bridge_config.c8y_prefix))
    }
}

impl From<&C8yMapperConfig> for C8YHttpConfig {
    fn from(config: &C8yMapperConfig) -> Self {
        C8YHttpConfig::new(
            config.device_id.clone(),
            config.c8y_host.clone(),
            config.c8y_mqtt.clone(),
            ProxyUrlGenerator::new(
                config.auth_proxy_addr.clone(),
                config.auth_proxy_port,
                config.auth_proxy_protocol,
            ),
        )
    }
}

pub struct BridgeConfig {
    pub c8y_prefix: TopicPrefix,
}

impl Record for BridgeConfig {
    fn extract_value(&self, path: &str) -> Option<Value> {
        match path {
            ".bridge.topic_prefix" => Some(self.c8y_prefix.as_str().into()),
            _ => None,
        }
    }
}

impl C8yPrefix for BridgeConfig {
    fn c8y_prefix(&self) -> &TopicPrefix {
        &self.c8y_prefix
    }
}

#[derive(Debug, thiserror::Error)]
pub enum C8yMapperConfigBuildError {
    #[error(transparent)]
    FromReadError(#[from] ReadError),

    #[error(transparent)]
    FromConfigNotSet(#[from] ConfigNotSet),

    #[error(transparent)]
    FromC8yMapperConfigError(#[from] C8yMapperConfigError),

    #[error(transparent)]
    FromTopicIdError(#[from] TopicIdError),

    #[error(transparent)]
    FromMultiError(#[from] MultiError),
}

#[derive(thiserror::Error, Debug)]
pub enum C8yMapperConfigError {
    #[error(transparent)]
    FromOperationsError(#[from] OperationsError),

    #[error(transparent)]
    FromMqttError(#[from] tedge_mqtt_ext::MqttError),
}
