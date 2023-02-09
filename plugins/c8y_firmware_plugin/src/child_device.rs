use crate::common::FirmwareEntry;
use crate::error::FirmwareManagementError;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use tedge_api::OperationStatus;

pub struct FirmwareOperationRequest {
    child_id: String,
    firmware_entry: FirmwareEntry,
}

pub struct FirmwareOperationResponse {
    child_id: String,
    payload: ChildDeviceResponsePayload,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChildDeviceRequestPayload {
    pub name: String,
    pub version: String,
    pub sha256: String,
    pub url: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChildDeviceResponsePayload {
    pub status: OperationStatus,
    pub name: String,
    pub version: String,
    pub url: String,
    pub reason: Option<String>,
}

impl FirmwareOperationResponse {
    pub fn get_child_id(&self) -> String {
        self.child_id.clone()
    }

    pub fn get_child_topic(&self) -> String {
        C8yTopic::ChildSmartRestResponse(self.child_id.clone()).to_string()
    }

    pub fn get_payload(&self) -> &ChildDeviceResponsePayload {
        &self.payload
    }
}

impl TryFrom<&Message> for FirmwareOperationResponse {
    type Error = FirmwareManagementError;

    fn try_from(message: &Message) -> Result<Self, Self::Error> {
        let topic = &message.topic.name;
        let child_id = get_child_id_from_child_topic(topic)?;
        let operation_name = get_operation_name_from_child_topic(topic)?;

        let request_payload: ChildDeviceResponsePayload =
            serde_json::from_str(message.payload_str()?)?;

        if operation_name == "firmware_update" {
            return Ok(Self {
                child_id,
                payload: request_payload,
            });
        }
        Err(FirmwareManagementError::InvalidTopicFromChildOperation {
            topic: topic.to_string(),
        })
    }
}

impl FirmwareOperationRequest {
    pub fn new(child_id: &str, firmware_entry: FirmwareEntry) -> FirmwareOperationRequest {
        FirmwareOperationRequest {
            child_id: child_id.to_string(),
            firmware_entry,
        }
    }

    pub fn operation_request_topic(&self) -> Topic {
        Topic::new_unchecked(&format!(
            "tedge/{}/commands/req/firmware_update",
            self.child_id
        ))
    }

    pub fn operation_request_payload(&self, url: &str) -> Result<String, anyhow::Error> {
        let request = ChildDeviceRequestPayload {
            name: self.firmware_entry.name.to_string(),
            version: self.firmware_entry.version.to_string(),
            sha256: self.firmware_entry.sha256.to_string(),
            url: url.to_string(),
        };
        Ok(serde_json::to_string(&request)?)
    }
}

pub fn get_child_id_from_child_topic(topic: &str) -> Result<String, FirmwareManagementError> {
    let mut topic_split = topic.split('/');
    // the second element is the child id
    let child_id =
        topic_split
            .nth(1)
            .ok_or(FirmwareManagementError::InvalidTopicFromChildOperation {
                topic: topic.into(),
            })?;
    Ok(child_id.to_string())
}

pub fn get_operation_name_from_child_topic(topic: &str) -> Result<String, FirmwareManagementError> {
    let topic_split = topic.split('/');
    let operation_name =
        topic_split
            .last()
            .ok_or(FirmwareManagementError::InvalidTopicFromChildOperation {
                topic: topic.into(),
            })?;
    Ok(operation_name.to_string())
}
