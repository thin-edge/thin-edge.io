use c8y_api::smartrest::error::OperationsError;
use c8y_api::smartrest::operations::Operations;
use c8y_api::smartrest::topic::C8yTopic;
use camino::Utf8PathBuf;
use std::path::Path;
use std::path::PathBuf;
use tedge_api::topic::ResponseTopic;
use tedge_config::C8yHttpSetting;
use tedge_config::ConfigSettingAccessor;
use tedge_config::DeviceIdSetting;
use tedge_config::DeviceTypeSetting;
use tedge_config::LogPathSetting;
use tedge_config::ServiceTypeSetting;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigError;
use tedge_mqtt_ext::TopicFilter;

pub const MQTT_MESSAGE_SIZE_THRESHOLD: usize = 16184;

pub struct C8yMapperConfig {
    pub config_dir: PathBuf,
    pub logs_path: Utf8PathBuf,
    pub device_id: String,
    pub device_type: String,
    pub service_type: String,
    pub ops_dir: PathBuf,
    pub c8y_host: String,
}

impl C8yMapperConfig {
    pub fn new(
        config_dir: PathBuf,
        logs_path: Utf8PathBuf,
        device_id: String,
        device_type: String,
        service_type: String,
        c8y_host: String,
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
        }
    }

    pub fn from_tedge_config(
        config_dir: impl AsRef<Path>,
        tedge_config: &TEdgeConfig,
    ) -> Result<C8yMapperConfig, TEdgeConfigError> {
        let config_dir: PathBuf = config_dir.as_ref().into();

        let logs_path = tedge_config.query(LogPathSetting)?;
        let device_id = tedge_config.query(DeviceIdSetting)?;
        let device_type = tedge_config.query(DeviceTypeSetting)?;
        let service_type = tedge_config.query(ServiceTypeSetting)?;
        let c8y_host = tedge_config.query(C8yHttpSetting)?.into();

        Ok(C8yMapperConfig::new(
            config_dir,
            logs_path,
            device_id,
            device_type,
            service_type,
            c8y_host,
        ))
    }

    pub fn subscriptions(config_dir: &Path) -> Result<TopicFilter, C8yMapperConfigError> {
        let operations = Operations::try_new(config_dir.join("operations/c8y"))?;
        let mut topic_filter: TopicFilter = vec![
            "tedge/measurements",
            "tedge/measurements/+",
            "tedge/alarms/+/+",
            "tedge/alarms/+/+/+",
            "c8y-internal/alarms/+/+",
            "c8y-internal/alarms/+/+/+",
            "tedge/events/+",
            "tedge/events/+/+",
            "tedge/health/+",
            "tedge/health/+/+",
            C8yTopic::SmartRestRequest.to_string().as_str(),
            ResponseTopic::SoftwareListResponse.as_str(),
            ResponseTopic::SoftwareUpdateResponse.as_str(),
            ResponseTopic::RestartResponse.as_str(),
        ]
        .try_into()
        .expect("topics that mapper should subscribe to");

        for topic in operations.topics_for_operations() {
            topic_filter.add(&topic)?;
        }

        Ok(topic_filter)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum C8yMapperConfigError {
    #[error(transparent)]
    FromOperationsError(#[from] OperationsError),

    #[error(transparent)]
    FromMqttError(#[from] tedge_mqtt_ext::MqttError),
}
