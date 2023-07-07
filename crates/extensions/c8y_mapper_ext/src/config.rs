use c8y_api::smartrest::error::OperationsError;
use c8y_api::smartrest::operations::Operations;
use c8y_api::smartrest::topic::C8yTopic;
use camino::Utf8PathBuf;
use std::path::Path;
use std::path::PathBuf;
use tedge_api::topic::ResponseTopic;
use tedge_config::new::ConfigNotSet;
use tedge_config::new::ReadError;
use tedge_config::new::TEdgeConfig;
use tedge_mqtt_ext::TopicFilter;
use tracing::log::warn;

pub const MQTT_MESSAGE_SIZE_THRESHOLD: usize = 16184;

pub struct C8yMapperConfig {
    pub config_dir: PathBuf,
    pub logs_path: Utf8PathBuf,
    pub device_id: String,
    pub device_type: String,
    pub service_type: String,
    pub ops_dir: PathBuf,
    pub c8y_host: String,
    pub topics: TopicFilter,
}

impl C8yMapperConfig {
    pub fn new(
        config_dir: PathBuf,
        logs_path: Utf8PathBuf,
        device_id: String,
        device_type: String,
        service_type: String,
        c8y_host: String,
        topics: TopicFilter,
    ) -> Self {
        let ops_dir = config_dir.join("operations").join("c8y");

        Self {
            config_dir,
            logs_path,
            device_id,
            device_type,
            service_type,
            ops_dir,
            c8y_host,
            topics,
        }
    }

    pub fn from_tedge_config(
        config_dir: impl AsRef<Path>,
        tedge_config: &TEdgeConfig,
    ) -> Result<C8yMapperConfig, C8yMapperConfigBuildError> {
        let config_dir: PathBuf = config_dir.as_ref().into();

        let logs_path = tedge_config.logs.path.clone();
        let device_id = tedge_config.device.id.try_read(tedge_config)?.to_string();
        let device_type = tedge_config.device.ty.clone();
        let service_type = tedge_config.service.ty.clone();
        let c8y_host = tedge_config.c8y_url().or_config_not_set()?.to_string();

        // The topics to subscribe = default internal topics + user configurable external topics
        let mut topics = Self::internal_topic_filter(&config_dir)?;
        for topic in tedge_config.c8y.topics.0.clone() {
            if topics.add(&topic).is_err() {
                warn!("The configured topic '{topic}' is invalid and ignored.");
            }
        }

        Ok(C8yMapperConfig::new(
            config_dir,
            logs_path,
            device_id,
            device_type,
            service_type,
            c8y_host,
            topics,
        ))
    }

    pub fn internal_topic_filter(config_dir: &Path) -> Result<TopicFilter, C8yMapperConfigError> {
        let mut topic_filter: TopicFilter = vec![
            "c8y-internal/alarms/+/+",
            "c8y-internal/alarms/+/+/+",
            C8yTopic::SmartRestRequest.to_string().as_str(),
            ResponseTopic::SoftwareListResponse.as_str(),
            ResponseTopic::SoftwareUpdateResponse.as_str(),
            ResponseTopic::RestartResponse.as_str(),
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
    pub fn default_external_topic_filter() -> TopicFilter {
        vec![
            "tedge/measurements",
            "tedge/measurements/+",
            "tedge/alarms/+/+",
            "tedge/alarms/+/+/+",
            "tedge/events/+",
            "tedge/events/+/+",
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
