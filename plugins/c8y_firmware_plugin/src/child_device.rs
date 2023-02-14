use crate::common::FirmwareOperationEntry;
use crate::error::FirmwareManagementError;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use tedge_api::OperationStatus;

#[derive(Debug)]
pub struct FirmwareOperationRequest {
    child_id: String,
    payload: ChildDeviceRequestPayload,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ChildDeviceRequestPayload {
    #[serde(rename = "id")]
    pub operation_id: String,
    pub attempt: usize,
    pub name: String,
    pub version: String,
    pub sha256: String,
    #[serde(rename = "url")]
    pub file_transfer_url: String,
}

impl FirmwareOperationRequest {
    pub fn new(operation_entry: FirmwareOperationEntry) -> Self {
        Self {
            child_id: operation_entry.child_id.to_string(),
            payload: ChildDeviceRequestPayload {
                operation_id: operation_entry.operation_id.to_string(),
                attempt: operation_entry.attempt,
                name: operation_entry.name.to_string(),
                version: operation_entry.version.to_string(),
                sha256: operation_entry.sha256.to_string(),
                file_transfer_url: operation_entry.file_transfer_url,
            },
        }
    }

    pub fn get_topic(&self) -> Topic {
        Topic::new_unchecked(&format!(
            "tedge/{}/commands/req/firmware_update",
            self.child_id
        ))
    }

    pub fn get_json_payload(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.payload)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct FirmwareOperationResponse {
    child_id: String,
    payload: ChildDeviceResponsePayload,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
pub struct ChildDeviceResponsePayload {
    #[serde(rename = "id")]
    pub operation_id: String,
    pub status: OperationStatus,
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
            Ok(Self {
                child_id,
                payload: request_payload,
            })
        } else {
            Err(FirmwareManagementError::InvalidTopicFromChildOperation {
                topic: topic.to_string(),
            })
        }
    }
}

// FIXME: Duplicated with config plugin
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

// FIXME: Duplicated with config plugin
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

#[cfg(test)]
mod tests {
    use super::*;

    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    #[test]
    fn create_firmware_operation_request() {
        let operation_entry = FirmwareOperationEntry {
            operation_id: "op-id".to_string(),
            child_id: "child-id".to_string(),
            name: "fw-name".to_string(),
            version: "fw-version".to_string(),
            server_url: "server-url".to_string(),
            file_transfer_url: "file-transfer-url".to_string(),
            sha256: "abcd1234".to_string(),
            attempt: 1,
        };
        let firmware_operation_request = FirmwareOperationRequest::new(operation_entry);

        let topic_to_publish = firmware_operation_request.get_topic();
        assert_eq!(
            topic_to_publish,
            Topic::new_unchecked("tedge/child-id/commands/req/firmware_update")
        );

        let json_request_payload = firmware_operation_request.get_json_payload().unwrap();
        let expected_json = json!({
            "id": "op-id",
            "name": "fw-name",
            "version": "fw-version",
            "sha256": "abcd1234",
            "url": "file-transfer-url",
            "attempt": 1
        });
        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&json_request_payload).unwrap(),
            expected_json
        );
    }

    #[test]
    fn create_firmware_operation_response() {
        let coming_payload = json!({
            "status": "executing",
            "id": "op-id",
            "reason": null
        })
        .to_string();
        let message = Message::new(
            &Topic::new_unchecked("tedge/child-id/commands/res/firmware_update"),
            coming_payload,
        );
        let firmware_response = FirmwareOperationResponse::try_from(&message).unwrap();

        let expected_payload = ChildDeviceResponsePayload {
            operation_id: "op-id".to_string(),
            status: OperationStatus::Executing,
            reason: None,
        };

        assert_eq!(firmware_response.get_payload(), &expected_payload);
        assert_eq!(firmware_response.get_child_id(), "child-id");
        assert_eq!(firmware_response.get_child_topic(), "c8y/s/us/child-id");
        assert_eq!(
            firmware_response,
            FirmwareOperationResponse {
                child_id: "child-id".to_string(),
                payload: expected_payload
            }
        );
    }
}
