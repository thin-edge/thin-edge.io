use crate::error::FirmwareManagementConfigBuildError;
use crate::error::FirmwareManagementError;

use c8y_api::http_proxy::C8yEndPoint;
use c8y_api::smartrest::topic::C8yTopic;
use camino::Utf8PathBuf;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tedge_api::path::DataDir;
use tedge_config::models::TopicPrefix;
use tedge_config::tedge_toml::mapper_config::C8yMapperConfig;
use tedge_config::TEdgeConfig;
use tedge_mqtt_ext::TopicFilter;

const FIRMWARE_UPDATE_RESPONSE_TOPICS: &str = "tedge/+/commands/res/firmware_update";

/// Configuration of the Firmware Manager
#[derive(Clone, Debug)]
pub struct FirmwareManagerConfig {
    pub tedge_device_id: String,
    pub local_http_host: Arc<str>,
    pub tmp_dir: Utf8PathBuf,
    pub data_dir: DataDir,
    pub c8y_request_topics: TopicFilter,
    pub firmware_update_response_topics: TopicFilter,
    pub timeout_sec: Duration,
    pub c8y_end_point: C8yEndPoint,
    pub c8y_prefix: TopicPrefix,
}

impl FirmwareManagerConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tedge_device_id: String,
        local_http_host: Arc<str>,
        local_http_port: u16,
        tmp_dir: Utf8PathBuf,
        data_dir: DataDir,
        timeout_sec: Duration,
        c8y_prefix: TopicPrefix,
        c8y_end_point: C8yEndPoint,
    ) -> Self {
        let local_http_host = format!("{}:{}", local_http_host, local_http_port).into();

        let c8y_request_topics = C8yTopic::SmartRestRequest.to_topic_filter(&c8y_prefix);
        let firmware_update_response_topics =
            TopicFilter::new_unchecked(FIRMWARE_UPDATE_RESPONSE_TOPICS);

        Self {
            tedge_device_id,
            local_http_host,
            tmp_dir,
            data_dir,
            c8y_request_topics,
            firmware_update_response_topics,
            timeout_sec,
            c8y_end_point,
            c8y_prefix,
        }
    }

    pub fn from_tedge_config(
        tedge_config: &TEdgeConfig,
        c8y_config: &C8yMapperConfig,
    ) -> Result<Self, FirmwareManagementConfigBuildError> {
        let tedge_device_id = c8y_config.device.id()?.to_string();
        let local_http_address = tedge_config.http.client.host.clone();
        let local_http_port = tedge_config.http.client.port;
        let tmp_dir = tedge_config.tmp.path.clone().into();
        let data_dir = tedge_config.data.path.as_path().to_owned().into();
        let timeout_sec = tedge_config.firmware.child.update.timeout.duration();

        let c8y_prefix = c8y_config.bridge.topic_prefix.clone();
        let c8y_end_point = C8yEndPoint::from_config(c8y_config)?;

        Ok(Self::new(
            tedge_device_id,
            local_http_address,
            local_http_port,
            tmp_dir,
            data_dir,
            timeout_sec,
            c8y_prefix,
            c8y_end_point,
        ))
    }

    // It checks the directory exists in the system
    pub fn validate_and_get_cache_dir_path(&self) -> Result<Utf8PathBuf, FirmwareManagementError> {
        validate_dir_exists(self.data_dir.cache_dir().as_path())?;
        Ok(self.data_dir.cache_dir().clone())
    }

    // It checks the directory exists in the system
    pub fn validate_and_get_file_transfer_dir_path(
        &self,
    ) -> Result<Utf8PathBuf, FirmwareManagementError> {
        validate_dir_exists(self.data_dir.file_transfer_dir().as_path())?;
        Ok(self.data_dir.file_transfer_dir().clone())
    }

    // It checks the directory exists in the system
    pub fn validate_and_get_firmware_dir_path(
        &self,
    ) -> Result<Utf8PathBuf, FirmwareManagementError> {
        validate_dir_exists(self.data_dir.firmware_dir().as_path())?;
        Ok(self.data_dir.firmware_dir().clone())
    }
}

fn validate_dir_exists(dir_path: impl AsRef<Path>) -> Result<(), FirmwareManagementError> {
    if dir_path.as_ref().is_dir() {
        Ok(())
    } else {
        Err(FirmwareManagementError::DirectoryNotFound {
            path: dir_path.as_ref().to_path_buf(),
        })
    }
}
