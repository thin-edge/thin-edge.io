use crate::error::FirmwareManagementError;

use c8y_api::http_proxy::C8yEndPoint;
use c8y_api::smartrest::topic::C8yTopic;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tedge_api::health::health_check_topics;
use tedge_config::C8yHttpSetting;
use tedge_config::ConfigSettingAccessor;
use tedge_config::DataPathSetting;
use tedge_config::DeviceIdSetting;
use tedge_config::FirmwareChildUpdateTimeoutSetting;
use tedge_config::HttpBindAddressSetting;
use tedge_config::HttpPortSetting;
use tedge_config::IpAddress;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigError;
use tedge_config::TmpPathSetting;
use tedge_mqtt_ext::TopicFilter;

const PLUGIN_SERVICE_NAME: &str = "c8y-firmware-plugin";
const FIRMWARE_UPDATE_RESPONSE_TOPICS: &str = "tedge/+/commands/res/firmware_update";

/// Configuration of the Firmware Manager
#[derive(Debug)]
pub struct FirmwareManagerConfig {
    pub tedge_device_id: String,
    pub local_http_host: String,
    pub tmp_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub file_transfer_dir: PathBuf,
    pub firmware_dir: PathBuf,
    pub c8y_request_topics: TopicFilter,
    pub health_check_topics: TopicFilter,
    pub firmware_update_response_topics: TopicFilter,
    pub timeout_sec: Duration,
    pub c8y_end_point: C8yEndPoint,
}

impl FirmwareManagerConfig {
    pub fn new(
        tedge_device_id: String,
        local_http_address: IpAddress,
        local_http_port: u16,
        tmp_dir: PathBuf,
        data_dir: PathBuf,
        timeout_sec: Duration,
        c8y_url: String,
    ) -> Self {
        let local_http_host = format!("{}:{}", local_http_address, local_http_port);

        let cache_dir = data_dir.join("cache");
        let file_transfer_dir = data_dir.join("file-transfer");
        let firmware_dir = data_dir.join("firmware");

        let c8y_request_topics = C8yTopic::SmartRestRequest.into();
        let health_check_topics = health_check_topics(PLUGIN_SERVICE_NAME);
        let firmware_update_response_topics =
            TopicFilter::new_unchecked(FIRMWARE_UPDATE_RESPONSE_TOPICS);

        let c8y_end_point = C8yEndPoint::new(&c8y_url, &tedge_device_id, "not used");

        Self {
            tedge_device_id,
            local_http_host,
            tmp_dir,
            data_dir,
            cache_dir,
            file_transfer_dir,
            firmware_dir,
            c8y_request_topics,
            health_check_topics,
            firmware_update_response_topics,
            timeout_sec,
            c8y_end_point,
        }
    }

    pub fn from_tedge_config(tedge_config: &TEdgeConfig) -> Result<Self, TEdgeConfigError> {
        let tedge_device_id = tedge_config.query(DeviceIdSetting)?;
        let local_http_address = tedge_config.query(HttpBindAddressSetting)?;
        let local_http_port: u16 = tedge_config.query(HttpPortSetting)?.into();
        let tmp_dir = tedge_config.query(TmpPathSetting)?.into();
        let data_dir = tedge_config.query(DataPathSetting)?.into();
        let timeout_sec = Duration::from_secs(
            tedge_config
                .query(FirmwareChildUpdateTimeoutSetting)?
                .into(),
        );
        let c8y_url = tedge_config.query(C8yHttpSetting)?.into();

        Ok(Self::new(
            tedge_device_id,
            local_http_address,
            local_http_port,
            tmp_dir,
            data_dir,
            timeout_sec,
            c8y_url,
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
        Err(FirmwareManagementError::DirectoryNotFound {
            path: dir_path.to_path_buf(),
        })
    }
}
