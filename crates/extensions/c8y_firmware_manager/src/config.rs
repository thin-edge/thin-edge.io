use crate::error::FirmwareManagementError;
use crate::error::FirmwareManagementError::DirectoryNotFound;

use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::TopicFilter;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tedge_api::health::health_check_topics;
use tedge_config::ConfigSettingAccessor;
use tedge_config::DeviceIdSetting;
use tedge_config::FirmwareChildUpdateTimeoutSetting;
use tedge_config::HttpBindAddressSetting;
use tedge_config::HttpPortSetting;
use tedge_config::IpAddress;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigError;
use tedge_config::TmpPathSetting;

const PLUGIN_SERVICE_NAME: &str = "c8y-firmware-plugin";
const FIRMWARE_UPDATE_RESPONSE_TOPICS: &str = "tedge/+/commands/res/firmware_update";

/// Configuration of the Firmware Manager
#[derive(Clone, Debug)]
pub struct FirmwareManagerConfig {
    pub tedge_device_id: String,
    pub local_http_host: String,
    pub tmp_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub file_transfer_dir: PathBuf,
    pub firmware_dir: PathBuf,
    pub c8y_request_topics: TopicFilter,
    pub health_check_topics: TopicFilter,
    pub firmware_update_response_topics: TopicFilter,
    pub timeout_sec: Duration,
}

impl FirmwareManagerConfig {
    pub fn new(
        tedge_device_id: String,
        local_http_address: IpAddress,
        local_http_port: u16,
        tmp_dir: PathBuf,
        persistent_dir: PathBuf,
        timeout_sec: Duration,
    ) -> Self {
        let local_http_host = format!("{}:{}", local_http_address, local_http_port);

        let cache_dir = persistent_dir.join("cache");
        let file_transfer_dir = persistent_dir.join("file-transfer");
        let firmware_dir = persistent_dir.join("firmware");

        let c8y_request_topics = C8yTopic::SmartRestRequest.into();
        let health_check_topics = health_check_topics(PLUGIN_SERVICE_NAME);
        let firmware_update_response_topics =
            TopicFilter::new_unchecked(FIRMWARE_UPDATE_RESPONSE_TOPICS);

        Self {
            tedge_device_id,
            local_http_host,
            tmp_dir,
            cache_dir,
            file_transfer_dir,
            firmware_dir,
            c8y_request_topics,
            health_check_topics,
            firmware_update_response_topics,
            timeout_sec,
        }
    }

    pub fn from_tedge_config(tedge_config: &TEdgeConfig) -> Result<Self, TEdgeConfigError> {
        let tedge_device_id = tedge_config.query(DeviceIdSetting)?;
        let local_http_address = tedge_config.query(HttpBindAddressSetting)?;
        let local_http_port: u16 = tedge_config.query(HttpPortSetting)?.into();
        let tmp_dir = tedge_config.query(TmpPathSetting)?.into();
        // FIXME: After Albin's PR merge
        let persistent_dir = PathBuf::from("/var/tedge");
        let timeout_sec = Duration::from_secs(
            tedge_config
                .query(FirmwareChildUpdateTimeoutSetting)?
                .into(),
        );

        Ok(Self::new(
            tedge_device_id,
            local_http_address,
            local_http_port,
            tmp_dir,
            persistent_dir,
            timeout_sec,
        ))
    }

    // It checks the directory exists in the system
    pub fn validate_and_get_cache_dir_path(&self) -> Result<PathBuf, FirmwareManagementError> {
        validate_dir_exists(self.cache_dir.as_path())?;
        Ok(self.cache_dir.clone())
    }

    // It checks the directory exists in the system
    pub fn validate_and_get_file_transfer_dir_path(
        &self,
    ) -> Result<PathBuf, FirmwareManagementError> {
        validate_dir_exists(self.file_transfer_dir.as_path())?;
        Ok(self.file_transfer_dir.clone())
    }

    // It checks the directory exists in the system
    pub fn validate_and_get_firmware_dir_path(&self) -> Result<PathBuf, FirmwareManagementError> {
        validate_dir_exists(self.firmware_dir.as_path())?;
        Ok(self.firmware_dir.clone())
    }
}

fn validate_dir_exists(dir_path: &Path) -> Result<(), FirmwareManagementError> {
    if dir_path.is_dir() {
        Ok(())
    } else {
        Err(DirectoryNotFound {
            path: dir_path.to_path_buf(),
        })
    }
}