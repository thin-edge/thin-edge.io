use crate::Capabilities;
use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
use c8y_api::smartrest::error::OperationsError;
use c8y_api::smartrest::operations::Operations;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_auth_proxy::url::Protocol;
use camino::Utf8Path;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use tedge_api::mqtt_topics::ChannelFilter::Command;
use tedge_api::mqtt_topics::ChannelFilter::CommandMetadata;
use tedge_api::mqtt_topics::EntityFilter::AnyEntity;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::mqtt_topics::TopicIdError;
use tedge_api::path::DataDir;
use tedge_config::AutoLogUpload;
use tedge_config::ConfigNotSet;
use tedge_config::ReadError;
use tedge_config::SoftwareManagementApiFlag;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigReaderService;
use tedge_config::TopicPrefix;
use tedge_mqtt_ext::TopicFilter;
use tracing::log::warn;

pub const MQTT_MESSAGE_SIZE_THRESHOLD: usize = 16184;
const STATE_DIR_NAME: &str = ".tedge-mapper-c8y";
const C8Y_CLOUD: &str = "c8y";
const SUPPORTED_OPERATIONS_DIRECTORY: &str = "operations";

pub struct C8yMapperConfig {
    pub device_id: String,
    pub device_topic_id: EntityTopicId,
    pub device_type: String,
    pub service: TEdgeConfigReaderService,
    pub c8y_host: String,
    pub tedge_http_host: Arc<str>,
    pub topics: TopicFilter,
    pub capabilities: Capabilities,
    pub auth_proxy_addr: Arc<str>,
    pub auth_proxy_port: u16,
    pub auth_proxy_protocol: Protocol,
    pub mqtt_schema: MqttSchema,
    pub enable_auto_register: bool,
    pub clean_start: bool,
    pub c8y_prefix: TopicPrefix,
    pub bridge_in_mapper: bool,
    pub software_management_api: SoftwareManagementApiFlag,
    pub software_management_with_types: bool,
    pub auto_log_upload: AutoLogUpload,
    pub identity: Option<tedge_api::download::Identity>,

    pub data_dir: DataDir,
    pub config_dir: Arc<Utf8Path>,
    pub logs_path: Arc<Utf8Path>,
    pub ops_dir: Arc<Utf8Path>,
    pub state_dir: Arc<Utf8Path>,
    pub tmp_dir: Arc<Utf8Path>,
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
        tedge_http_host: Arc<str>,
        topics: TopicFilter,
        capabilities: Capabilities,
        auth_proxy_addr: Arc<str>,
        auth_proxy_port: u16,
        auth_proxy_protocol: Protocol,
        mqtt_schema: MqttSchema,
        enable_auto_register: bool,
        clean_start: bool,
        c8y_prefix: TopicPrefix,
        bridge_in_mapper: bool,
        software_management_api: SoftwareManagementApiFlag,
        software_management_with_types: bool,
        auto_log_upload: AutoLogUpload,
        identity: Option<tedge_api::download::Identity>,
    ) -> Self {
        let ops_dir = config_dir
            .join(SUPPORTED_OPERATIONS_DIRECTORY)
            .join(C8Y_CLOUD)
            .into();
        let state_dir = config_dir.join(STATE_DIR_NAME).into();

        Self {
            data_dir,
            device_id,
            device_topic_id,
            device_type,
            service,
            c8y_host,
            tedge_http_host,
            topics,
            capabilities,
            auth_proxy_addr,
            auth_proxy_port,
            auth_proxy_protocol,
            mqtt_schema,
            enable_auto_register,
            clean_start,
            c8y_prefix,
            bridge_in_mapper,
            software_management_api,
            software_management_with_types,
            auto_log_upload,
            identity,

            config_dir,
            logs_path,
            ops_dir,
            state_dir,
            tmp_dir,
        }
    }

    pub fn bridge_service_name(&self) -> String {
        if self.bridge_in_mapper {
            format!("tedge-mapper-bridge-{}", self.c8y_prefix)
        } else {
            "mosquitto-c8y-bridge".into()
        }
    }

    pub fn from_tedge_config(
        config_dir: impl AsRef<Utf8Path>,
        tedge_config: &TEdgeConfig,
    ) -> Result<C8yMapperConfig, C8yMapperConfigBuildError> {
        let config_dir: Arc<Utf8Path> = config_dir.as_ref().into();

        let logs_path = tedge_config.logs.path.as_path().into();
        let data_dir: DataDir = tedge_config.data.path.clone().into();
        let tmp_dir = tedge_config.tmp.path.as_path().into();

        let device_id = tedge_config.device.id.try_read(tedge_config)?.to_string();
        let device_type = tedge_config.device.ty.clone();
        let device_topic_id = EntityTopicId::from_str(&tedge_config.mqtt.device_topic_id)?;
        let service = tedge_config.service.clone();
        let c8y_host = tedge_config.c8y.http.or_config_not_set()?.to_string();
        let tedge_http_address = tedge_config.http.client.host.clone();
        let tedge_http_port = tedge_config.http.client.port;
        let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
        let auth_proxy_addr = tedge_config.c8y.proxy.client.host.clone();
        let auth_proxy_port = tedge_config.c8y.proxy.client.port;
        let auth_proxy_protocol = tedge_config
            .c8y
            .proxy
            .cert_path
            .or_none()
            .map_or(Protocol::Http, |_| Protocol::Https);

        let tedge_http_host = format!("{}:{}", tedge_http_address, tedge_http_port).into();

        let capabilities = Capabilities {
            log_upload: tedge_config.c8y.enable.log_upload,
            config_snapshot: tedge_config.c8y.enable.config_snapshot,
            config_update: tedge_config.c8y.enable.config_update,
            firmware_update: tedge_config.c8y.enable.firmware_update,
        };
        let c8y_prefix = tedge_config.c8y.bridge.topic_prefix.clone();

        let mut topics =
            Self::default_internal_topic_filter(config_dir.as_std_path(), &c8y_prefix)?;
        let enable_auto_register = tedge_config.c8y.entity_store.auto_register;
        let clean_start = tedge_config.c8y.entity_store.clean_start;

        let software_management_api = tedge_config.c8y.software_management.api.clone();
        let software_management_with_types = tedge_config.c8y.software_management.with_types;

        let auto_log_upload = tedge_config.c8y.operations.auto_log_upload.clone();
        let identity = tedge_config.http.client.auth.identity()?;

        // Add feature topic filters
        for cmd in [
            OperationType::Restart,
            OperationType::SoftwareList,
            OperationType::SoftwareUpdate,
        ] {
            topics.add_all(mqtt_schema.topics(AnyEntity, Command(cmd.clone())));
            topics.add_all(mqtt_schema.topics(AnyEntity, CommandMetadata(cmd)));
        }

        if capabilities.log_upload {
            topics.add_all(crate::operations::log_upload::log_upload_topic_filter(
                &mqtt_schema,
            ));
        }
        if capabilities.config_snapshot {
            topics.add_all(crate::operations::config_snapshot::topic_filter(
                &mqtt_schema,
            ));
        }
        if capabilities.config_update {
            topics.add_all(crate::operations::config_update::topic_filter(&mqtt_schema));
        }
        if capabilities.firmware_update {
            topics.add_all(
                crate::operations::firmware_update::firmware_update_topic_filter(&mqtt_schema),
            );
        }

        // Add user configurable external topic filters
        for topic in tedge_config.c8y.topics.0.clone() {
            if topics.add(&topic).is_err() {
                warn!("The configured topic '{topic}' is invalid and ignored.");
            }
        }

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
            tedge_http_host,
            topics,
            capabilities,
            auth_proxy_addr,
            auth_proxy_port,
            auth_proxy_protocol,
            mqtt_schema,
            enable_auto_register,
            clean_start,
            c8y_prefix,
            bridge_in_mapper,
            software_management_api,
            software_management_with_types,
            auto_log_upload,
            identity,
        ))
    }

    pub fn default_internal_topic_filter(
        config_dir: &Path,
        prefix: &TopicPrefix,
    ) -> Result<TopicFilter, C8yMapperConfigError> {
        let mut topic_filter: TopicFilter = vec![
            "c8y-internal/alarms/+/+/+/+/+/a/+",
            C8yTopic::SmartRestRequest.with_prefix(prefix).as_str(),
            &C8yDeviceControlTopic::name(prefix),
        ]
        .try_into()
        .expect("topics that mapper should subscribe to");

        if let Ok(operations) = Operations::try_new(
            config_dir
                .join(SUPPORTED_OPERATIONS_DIRECTORY)
                .join(C8Y_CLOUD),
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
    OtherError(#[from] anyhow::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum C8yMapperConfigError {
    #[error(transparent)]
    FromOperationsError(#[from] OperationsError),

    #[error(transparent)]
    FromMqttError(#[from] tedge_mqtt_ext::MqttError),
}
