use crate::Capabilities;
use c8y_api::smartrest::error::OperationsError;
use c8y_api::smartrest::operations::Operations;
use c8y_api::smartrest::topic::C8yTopic;
use camino::Utf8PathBuf;
use std::net::IpAddr;
use std::path::Path;
use std::path::PathBuf;
use tedge_api::mqtt_topics::ChannelFilter::Command;
use tedge_api::mqtt_topics::ChannelFilter::CommandMetadata;
use tedge_api::mqtt_topics::EntityFilter::AnyEntity;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::path::DataDir;
use tedge_api::topic::ResponseTopic;
use tedge_config::ConfigNotSet;
use tedge_config::ReadError;
use tedge_config::TEdgeConfig;
use tedge_mqtt_ext::TopicFilter;
use tracing::log::warn;

pub const MQTT_MESSAGE_SIZE_THRESHOLD: usize = 16184;

pub struct C8yMapperConfig {
    pub config_dir: PathBuf,
    pub logs_path: Utf8PathBuf,
    pub data_dir: DataDir,
    pub device_id: String,
    pub device_type: String,
    pub service_type: String,
    pub ops_dir: PathBuf,
    pub c8y_host: String,
    pub tedge_http_host: String,
    pub topics: TopicFilter,
    pub capabilities: Capabilities,
    pub auth_proxy_addr: IpAddr,
    pub auth_proxy_port: u16,
}

impl C8yMapperConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config_dir: PathBuf,
        logs_path: Utf8PathBuf,
        data_dir: DataDir,
        device_id: String,
        device_type: String,
        service_type: String,
        c8y_host: String,
        tedge_http_host: String,
        topics: TopicFilter,
        capabilities: Capabilities,
        auth_proxy_addr: IpAddr,
        auth_proxy_port: u16,
    ) -> Self {
        let ops_dir = config_dir.join("operations").join("c8y");

        Self {
            config_dir,
            logs_path,
            data_dir,
            device_id,
            device_type,
            service_type,
            ops_dir,
            c8y_host,
            tedge_http_host,
            topics,
            capabilities,
            auth_proxy_addr,
            auth_proxy_port,
        }
    }

    pub fn from_tedge_config(
        config_dir: impl AsRef<Path>,
        tedge_config: &TEdgeConfig,
    ) -> Result<C8yMapperConfig, C8yMapperConfigBuildError> {
        let config_dir: PathBuf = config_dir.as_ref().into();

        let logs_path = tedge_config.logs.path.clone();
        let data_dir: DataDir = tedge_config.data.path.clone().into();
        let device_id = tedge_config.device.id.try_read(tedge_config)?.to_string();
        let device_type = tedge_config.device.ty.clone();
        let service_type = tedge_config.service.ty.clone();
        let c8y_host = tedge_config.c8y.http.or_config_not_set()?.to_string();
        let tedge_http_address = tedge_config.http.bind.address;
        let tedge_http_port = tedge_config.http.bind.port;
        let mqtt_schema = MqttSchema::default(); // later get the value from tedge config
        let auth_proxy_addr = tedge_config.c8y.proxy.bind.address;
        let auth_proxy_port = tedge_config.c8y.proxy.bind.port;

        let tedge_http_host = format!("{}:{}", tedge_http_address, tedge_http_port);

        let capabilities = Capabilities {
            log_management: tedge_config.c8y.enable.log_management,
            config_snapshot: true, // fix later
            config_update: true,
        };

        let mut topics = Self::default_internal_topic_filter(&config_dir)?;

        // Add feature topic filters
        topics.add_all(mqtt_schema.topics(AnyEntity, Command(OperationType::Restart)));
        topics.add_all(mqtt_schema.topics(AnyEntity, CommandMetadata(OperationType::Restart)));
        if capabilities.log_management {
            topics.add_all(crate::log_upload::log_upload_topic_filter(&mqtt_schema));
        }
        if capabilities.config_snapshot {
            crate::config_operations::config_snapshot_topic_filter(&mqtt_schema);
        }
        if capabilities.config_update {
            crate::config_operations::config_update_topic_filter(&mqtt_schema);
        }

        // Add user configurable external topic filters
        for topic in tedge_config.c8y.topics.0.clone() {
            if topics.add(&topic).is_err() {
                warn!("The configured topic '{topic}' is invalid and ignored.");
            }
        }

        Ok(C8yMapperConfig::new(
            config_dir,
            logs_path,
            data_dir,
            device_id,
            device_type,
            service_type,
            c8y_host,
            tedge_http_host,
            topics,
            capabilities,
            auth_proxy_addr,
            auth_proxy_port,
        ))
    }

    pub fn default_internal_topic_filter(
        config_dir: &Path,
    ) -> Result<TopicFilter, C8yMapperConfigError> {
        let mut topic_filter: TopicFilter = vec![
            "c8y-internal/alarms/+/+/+/+/+/a/+",
            C8yTopic::SmartRestRequest.to_string().as_str(),
            ResponseTopic::SoftwareListResponse.as_str(),
            ResponseTopic::SoftwareUpdateResponse.as_str(),
        ]
        .try_into()
        .expect("topics that mapper should subscribe to");

        if let Ok(operations) = Operations::try_new(config_dir.join("operations").join("c8y")) {
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
            "te/+/+/+/+/m/+",
            "te/+/+/+/+/e/+",
            "te/+/+/+/+/a/+",
            "tedge/health/+",
            "tedge/health/+/+",
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
}

#[derive(thiserror::Error, Debug)]
pub enum C8yMapperConfigError {
    #[error(transparent)]
    FromOperationsError(#[from] OperationsError),

    #[error(transparent)]
    FromMqttError(#[from] tedge_mqtt_ext::MqttError),
}
