use c8y_api::json_c8y::*;
use std::path::PathBuf;
use tedge_actors::fan_in_message_type;

fan_in_message_type!(C8YRestRequest[C8yCreateEvent, C8yUpdateSoftwareListResponse, UploadLogBinary, UploadConfigFile]: Debug);
fan_in_message_type!(C8YRestResponse[EventId, Unit]: Debug);

#[derive(Debug)]
pub struct UploadLogBinary {
    log_type: String,
    log_content: String,
    child_device_id: Option<String>,
}

#[derive(Debug)]
pub struct UploadConfigFile {
    config_path: PathBuf,
    config_type: String,
    child_device_id: Option<String>,
}

pub type EventId = String;

pub type Unit = ();
