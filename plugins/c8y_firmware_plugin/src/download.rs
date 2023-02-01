use crate::error::FirmwareManagementError;
use crate::firmware_manager::ActiveOperationState;
use c8y_api::http_proxy::C8YHttpProxy;

use c8y_api::smartrest::smartrest_deserializer::SmartRestFirmwareRequest;
use download::DownloadInfo;
use mqtt_channel::Message;
use mqtt_channel::UnboundedSender;
use std::path::PathBuf;
use std::sync::Arc;
use tedge_utils::file::get_filename;
use tedge_utils::file::PermissionEntry;
use tedge_utils::timers::Timers;
use tokio::sync::Mutex;
use tracing::error;
use tracing::info;

pub struct FirmwareDownloadManager {
    tedge_device_id: String,
    mqtt_publisher: UnboundedSender<Message>,
    http_client: Arc<Mutex<dyn C8YHttpProxy>>,
    local_http_host: String,
    config_dir: PathBuf,
    tmp_dir: PathBuf,
    pub operation_timer: Timers<(String, String), ActiveOperationState>,
}

impl FirmwareDownloadManager {
    pub fn new(
        tedge_device_id: String,
        mqtt_publisher: UnboundedSender<Message>,
        http_client: Arc<Mutex<dyn C8YHttpProxy>>,
        local_http_host: String,
        config_dir: PathBuf,
        tmp_dir: PathBuf,
    ) -> Self {
        FirmwareDownloadManager {
            tedge_device_id,
            mqtt_publisher,
            http_client,
            local_http_host,
            config_dir,
            tmp_dir,
            operation_timer: Timers::new(),
        }
    }

    pub async fn handle_firmware_download_request(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
    ) -> Result<(), anyhow::Error> {
        info!(
            "Received c8y_Firmware request for device {}. Name: {}, Version: {}, URL: {}",
            smartrest_request.device,
            smartrest_request.name,
            smartrest_request.version,
            smartrest_request.url,
        );

        if smartrest_request.device == self.tedge_device_id {
            error!("This plugin does not support firmware request for the tedge device.");
            Ok(())
        } else {
            self.handle_firmware_download_request_child_device(smartrest_request)
                .await
        }
    }

    /// Map the c8y_Firmware request into a tedge/commands/req/firmware_update command for the child device.
    /// The firmware is shared with the child device via the file transfer service.
    /// The firmware is downloaded from Cumulocity and is uploaded to the file transfer service,
    /// so that it can be shared with a child device.
    /// A unique URL path for this firmware, from the file transfer service, is shared with the child device in the command.
    /// The child device can use this URL to download the firmware from the file transfer service.
    pub async fn handle_firmware_download_request_child_device(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
    ) -> Result<(), anyhow::Error> {
        unimplemented!();
    }
}
