use agent_interface::OperationStatus;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::{Message, Topic};

use crate::{config::FileEntry, error::ChildDeviceConfigManagementError};

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

    pub fn http_file_repository_relative_path(&self) -> String {
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

/// Return child id from topic.
pub fn get_child_id_from_child_topic(
    topic: &str,
) -> Result<String, ChildDeviceConfigManagementError> {
    let mut topic_split = topic.split('/');
    // the second element is the child id
    let child_id = topic_split.nth(1).ok_or(
        ChildDeviceConfigManagementError::InvalidTopicFromChildOperation {
            topic: topic.into(),
        },
    )?;
    Ok(child_id.to_string())
}

/// Return operation name from topic.
fn get_operation_name_from_child_topic(
    topic: &str,
) -> Result<String, ChildDeviceConfigManagementError> {
    let topic_split = topic.split('/');
    let operation_name = topic_split.last().ok_or(
        ChildDeviceConfigManagementError::InvalidTopicFromChildOperation {
            topic: topic.into(),
        },
    )?;
    Ok(operation_name.to_string())
}
impl TryFrom<&Message> for ConfigOperationResponse {
    type Error = ChildDeviceConfigManagementError;

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
        Err(
            ChildDeviceConfigManagementError::InvalidTopicFromChildOperation {
                topic: topic.to_string(),
            },
        )
    }
}

impl ConfigOperationRequest {
    pub fn http_file_repository_relative_path(&self) -> String {
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
    ) -> Result<String, ChildDeviceConfigManagementError> {
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
