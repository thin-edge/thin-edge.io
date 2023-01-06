use c8y_api::json_c8y::*;
use std::path::PathBuf;
use tedge_actors::fan_in_message_type;
use tedge_utils::file::PermissionEntry;

fan_in_message_type!(C8YRestRequest[C8yCreateEvent, C8yUpdateSoftwareListResponse, UploadLogBinary, UploadConfigFile, DownloadFile]: Debug);
fan_in_message_type!(C8YRestResponse[EventId, Unit]: Debug);

#[derive(Debug)]
pub struct UploadLogBinary {
    pub log_type: String,
    pub log_content: String,
    pub child_device_id: Option<String>,
}

#[derive(Debug)]
pub struct UploadConfigFile {
    pub config_path: PathBuf,
    pub config_type: String,
    pub child_device_id: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DownloadFile {
    pub download_url: String,
    pub file_path: PathBuf,
    pub file_permissions: PermissionEntry,
}

pub type EventId = String;

pub type Unit = ();
