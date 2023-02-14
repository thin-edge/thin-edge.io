use super::actor::ConfigOperation;
use super::error::ConfigManagementError;
use super::plugin_config::FileEntry;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use std::fs;
use std::time::Duration;
use tedge_api::OperationStatus;
use tracing::error;

#[cfg(test)]
pub const FILE_TRANSFER_ROOT_PATH: &str = "/tmp";
#[cfg(not(test))]
pub const FILE_TRANSFER_ROOT_PATH: &str = "/var/tedge/file-transfer";
pub const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(60); //TODO: Make this configurable?

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ConfigOperationResponseTopic {
    SnapshotResponse,
    UpdateResponse,
}

#[allow(clippy::from_over_into)]
// can not implement From since the topic can be anything (`new_unchecked` can be any &str)
impl Into<TopicFilter> for ConfigOperationResponseTopic {
    fn into(self) -> TopicFilter {
        match self {
            ConfigOperationResponseTopic::SnapshotResponse => {
                TopicFilter::new_unchecked("tedge/+/commands/res/config_snapshot")
            }
            ConfigOperationResponseTopic::UpdateResponse => {
                TopicFilter::new_unchecked("tedge/+/commands/res/config_update")
            }
        }
    }
}

pub trait ConfigOperationMessage {
    fn http_file_repository_relative_path(&self) -> String;

    fn file_transfer_repository_full_path(&self) -> String {
        format!(
            "{FILE_TRANSFER_ROOT_PATH}/{}",
            self.http_file_repository_relative_path()
        )
    }
}

/// A child device can receive the following operation requests:
///
/// - Update:
///
///     An operation that requests the child device to update its configuration with an update from the cloud.
///
/// - Snapshot:
///
///     An operation that requests the child device to upload its current configuration snapshot to the cloud.
pub enum ConfigOperationRequest {
    Update {
        child_id: String,
        file_entry: FileEntry,
    },
    Snapshot {
        child_id: String,
        file_entry: FileEntry,
    },
}

pub enum ConfigOperationResponse {
    Update {
        child_id: String,
        payload: ChildDeviceResponsePayload,
    },
    Snapshot {
        child_id: String,
        payload: ChildDeviceResponsePayload,
    },
}

impl ConfigOperationResponse {
    pub fn get_child_id(&self) -> String {
        match self {
            ConfigOperationResponse::Update { child_id, .. } => child_id.to_string(),
            ConfigOperationResponse::Snapshot { child_id, .. } => child_id.to_string(),
        }
    }

    pub fn get_config_type(&self) -> String {
        match self {
            ConfigOperationResponse::Update { payload, .. } => payload.config_type.to_string(),
            ConfigOperationResponse::Snapshot { payload, .. } => payload.config_type.to_string(),
        }
    }

    pub fn get_child_topic(&self) -> String {
        match self {
            ConfigOperationResponse::Update { child_id, .. } => {
                C8yTopic::ChildSmartRestResponse(child_id.to_owned()).to_string()
            }
            ConfigOperationResponse::Snapshot { child_id, .. } => {
                C8yTopic::ChildSmartRestResponse(child_id.to_owned()).to_string()
            }
        }
    }

    pub fn get_payload(&self) -> &ChildDeviceResponsePayload {
        match self {
            ConfigOperationResponse::Update { payload, .. } => payload,
            ConfigOperationResponse::Snapshot { payload, .. } => payload,
        }
    }
}

impl ConfigOperationMessage for ConfigOperationResponse {
    fn http_file_repository_relative_path(&self) -> String {
        match self {
            ConfigOperationResponse::Update {
                child_id, payload, ..
            } => {
                format!("{}/config_update/{}", child_id, payload.config_type)
            }
            ConfigOperationResponse::Snapshot {
                child_id, payload, ..
            } => {
                format!("{}/config_snapshot/{}", child_id, payload.config_type)
            }
        }
    }
}

pub fn try_cleanup_config_file_from_file_transfer_repositoy(
    config_response: &ConfigOperationResponse,
) {
    let config_file_path = config_response.file_transfer_repository_full_path();
    if let Err(err) = fs::remove_file(&config_file_path) {
        error!(
            "Failed to remove config file file copy at {} with {}",
            config_file_path, err
        );
    }
}

/// Return child id from topic.
pub fn get_child_id_from_child_topic(topic: &str) -> Result<String, ConfigManagementError> {
    let mut topic_split = topic.split('/');
    // the second element is the child id
    let child_id = topic_split
        .nth(1)
        .ok_or(ConfigManagementError::InvalidChildDeviceTopic {
            topic: topic.into(),
        })?;
    Ok(child_id.to_string())
}

/// Return operation name from topic.
pub fn get_operation_name_from_child_topic(topic: &str) -> Result<String, ConfigManagementError> {
    let topic_split = topic.split('/');
    let operation_name =
        topic_split
            .last()
            .ok_or(ConfigManagementError::InvalidChildDeviceTopic {
                topic: topic.into(),
            })?;
    Ok(operation_name.to_string())
}
impl TryFrom<&Message> for ConfigOperationResponse {
    type Error = ConfigManagementError;

    fn try_from(message: &Message) -> Result<Self, Self::Error> {
        let topic = &message.topic.name;
        let child_id = get_child_id_from_child_topic(topic)?;
        let operation_name = get_operation_name_from_child_topic(topic)?;

        let request_payload: ChildDeviceResponsePayload =
            serde_json::from_str(message.payload_str()?)?;

        if operation_name == "config_snapshot" {
            return Ok(Self::Snapshot {
                child_id,
                payload: request_payload,
            });
        }
        if operation_name == "config_update" {
            return Ok(Self::Update {
                child_id,
                payload: request_payload,
            });
        }
        Err(ConfigManagementError::InvalidChildDeviceTopic {
            topic: topic.to_string(),
        })
    }
}

impl ConfigOperationRequest {
    /// The configuration management topic for a child device.
    ///
    /// # Example:
    /// For a configuration update returns:
    ///     - "tedge/CHILD_ID/commands/req/config_update"
    ///
    /// For a configuration snapshot returns:
    ///     - "tedge/CHILD_ID/commands/req/config_snapshot"
    pub fn operation_request_topic(&self) -> Topic {
        match self {
            ConfigOperationRequest::Update { child_id, .. } => {
                Topic::new_unchecked(&format!("tedge/{}/commands/req/config_update", child_id))
            }
            ConfigOperationRequest::Snapshot { child_id, .. } => {
                Topic::new_unchecked(&format!("tedge/{}/commands/req/config_snapshot", child_id))
            }
        }
    }

    /// The configuration management payload for a child device.
    pub fn operation_request_payload(
        &self,
        local_http_host: &str,
    ) -> Result<String, ConfigManagementError> {
        let url = format!(
            "http://{local_http_host}/tedge/file-transfer/{}",
            self.http_file_repository_relative_path()
        );
        match self {
            ConfigOperationRequest::Update {
                child_id: _,
                file_entry,
            } => {
                let request = ChildDeviceRequestPayload {
                    url,
                    path: file_entry.path.clone(),
                    config_type: Some(file_entry.config_type.clone()),
                };
                Ok(serde_json::to_string(&request)?)
            }
            ConfigOperationRequest::Snapshot {
                child_id: _,
                file_entry,
            } => {
                let request = ChildDeviceRequestPayload {
                    url,
                    path: file_entry.path.clone(),
                    config_type: Some(file_entry.config_type.clone()),
                };
                Ok(serde_json::to_string(&request)?)
            }
        }
    }
}

impl ConfigOperationMessage for ConfigOperationRequest {
    fn http_file_repository_relative_path(&self) -> String {
        match self {
            ConfigOperationRequest::Update {
                child_id,
                file_entry,
                ..
            } => {
                format!("{}/config_update/{}", child_id, file_entry.config_type)
            }
            ConfigOperationRequest::Snapshot {
                child_id,
                file_entry,
                ..
            } => {
                format!("{}/config_snapshot/{}", child_id, file_entry.config_type)
            }
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChildDeviceResponsePayload {
    pub status: Option<OperationStatus>,
    pub path: String,
    #[serde(rename = "type")]
    pub config_type: String,
    pub reason: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChildDeviceRequestPayload {
    pub url: String,
    pub path: String,
    #[serde(rename = "type")]
    pub config_type: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct ChildConfigOperationKey {
    pub child_id: String,
    pub operation_type: ConfigOperation,
    pub config_type: String,
}
