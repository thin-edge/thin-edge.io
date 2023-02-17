use crate::error::FirmwareManagementError;
use crate::firmware_manager::FirmwareOperationEntry;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use serde::Deserialize;
use serde::Deserializer;
use tedge_api::OperationStatus;

#[derive(Debug)]
pub struct FirmwareOperationRequest {
    child_id: String,
    payload: RequestPayload,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct RequestPayload {
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
    fn get_topic(&self) -> Topic {
        Topic::new_unchecked(&format!(
            "tedge/{}/commands/req/firmware_update",
            self.child_id
        ))
    }

    fn get_json_payload(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.payload)
    }
}

impl From<FirmwareOperationEntry> for FirmwareOperationRequest {
    fn from(entry: FirmwareOperationEntry) -> Self {
        Self {
            child_id: entry.child_id.to_string(),
            payload: RequestPayload {
                operation_id: entry.operation_id.to_string(),
                attempt: entry.attempt,
                name: entry.name.to_string(),
                version: entry.version.to_string(),
                sha256: entry.sha256.to_string(),
                file_transfer_url: entry.file_transfer_url,
            },
        }
    }
}

impl TryInto<Message> for FirmwareOperationRequest {
    type Error = FirmwareManagementError;

    fn try_into(self) -> Result<Message, Self::Error> {
        let message = Message::new(&self.get_topic(), self.get_json_payload()?);
        Ok(message)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct FirmwareOperationResponse {
    child_id: String,
    payload: ResponsePayload,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
pub struct ResponsePayload {
    #[serde(rename = "id")]
    pub operation_id: String,
    #[serde(default, deserialize_with = "treat_error_as_none")]
    pub status: Option<OperationStatus>,
    pub reason: Option<String>,
}

fn treat_error_as_none<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Ok(Option::deserialize(deserializer).unwrap_or(None))
}

impl FirmwareOperationResponse {
    pub fn get_child_id(&self) -> String {
        self.child_id.clone()
    }

    pub fn get_child_topic(&self) -> String {
        C8yTopic::ChildSmartRestResponse(self.child_id.clone()).to_string()
    }

    pub fn get_payload(&self) -> &ResponsePayload {
        &self.payload
    }
}

impl TryFrom<&Message> for FirmwareOperationResponse {
    type Error = FirmwareManagementError;

    fn try_from(message: &Message) -> Result<Self, Self::Error> {
        let topic = &message.topic.name;
        let child_id = get_child_id_from_child_topic(topic)?;
        let request_payload: ResponsePayload = serde_json::from_str(message.payload_str()?)?;

        if request_payload.status.is_none() {
            return Err(FirmwareManagementError::InvalidOperationStatus {
                op_id: request_payload.operation_id,
            });
        }

        Ok(Self {
            child_id,
            payload: request_payload,
        })
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
        let firmware_operation_request = FirmwareOperationRequest::from(operation_entry);

        let message: Message = firmware_operation_request.try_into().unwrap();
        assert_eq!(
            message.topic,
            Topic::new_unchecked("tedge/child-id/commands/req/firmware_update")
        );

        let expected_json = json!({
            "id": "op-id",
            "name": "fw-name",
            "version": "fw-version",
            "sha256": "abcd1234",
            "url": "file-transfer-url",
            "attempt": 1
        });
        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&message.payload_str().unwrap()).unwrap(),
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

        let expected_payload = ResponsePayload {
            operation_id: "op-id".to_string(),
            status: Some(OperationStatus::Executing),
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

    #[test]
    fn deserialize_response_payload() {
        let payload = json!({
            "status": "failed",
            "id": "op-id",
            "reason": "aaa"
        })
        .to_string();
        let request_payload: ResponsePayload = serde_json::from_str(&payload).unwrap();
        let expected_response_payload = ResponsePayload {
            operation_id: "op-id".to_string(),
            status: Some(OperationStatus::Failed),
            reason: Some("aaa".to_string()),
        };
        assert_eq!(request_payload, expected_response_payload);
    }

    #[test]
    fn deserialize_response_payload_with_only_operation_id() {
        let payload = json!({
            "id": "op-id",
        })
        .to_string();
        let request_payload: ResponsePayload = serde_json::from_str(&payload).unwrap();
        let expected_response_payload = ResponsePayload {
            operation_id: "op-id".to_string(),
            status: None,
            reason: None,
        };
        assert_eq!(request_payload, expected_response_payload);
    }

    #[test]
    fn deserialize_response_payload_with_invalid_operation_status() {
        let payload = json!({
            "status": "invalid",
            "id": "op-id",
        })
        .to_string();
        let request_payload: ResponsePayload = serde_json::from_str(&payload).unwrap();
        let expected_response_payload = ResponsePayload {
            operation_id: "op-id".to_string(),
            status: None,
            reason: None,
        };
        assert_eq!(request_payload, expected_response_payload);
    }

    #[test]
    fn deserialize_response_payload_without_operation_id() {
        let payload = json!({
            "status": "executing",
            "reason": "aaa"
        })
        .to_string();
        let result = serde_json::from_str::<ResponsePayload>(&payload);
        assert!(result.is_err())
    }
}
