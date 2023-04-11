use crate::error::FirmwareManagementError;
use crate::operation::FirmwareOperationEntry;

use c8y_api::smartrest::error::SmartRestSerializerError;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRest;
use c8y_api::smartrest::smartrest_serializer::SmartRestSerializer;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToExecuting;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToFailed;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToSuccessful;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use tedge_api::topic::get_child_id_from_child_topic;
use tedge_api::OperationStatus;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;

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

impl TryInto<MqttMessage> for FirmwareOperationRequest {
    type Error = FirmwareManagementError;

    fn try_into(self) -> Result<MqttMessage, Self::Error> {
        let message = MqttMessage::new(&self.get_topic(), self.get_json_payload()?);
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
    pub status: OperationStatus,
    pub reason: Option<String>,
}

impl FirmwareOperationResponse {
    pub fn get_child_id(&self) -> String {
        self.child_id.clone()
    }

    pub fn get_payload(&self) -> &ResponsePayload {
        &self.payload
    }
}

impl TryFrom<&MqttMessage> for FirmwareOperationResponse {
    type Error = FirmwareManagementError;

    fn try_from(message: &MqttMessage) -> Result<Self, Self::Error> {
        let topic = &message.topic.name;
        let child_id = get_child_id_from_child_topic(topic).ok_or(
            FirmwareManagementError::InvalidTopicFromChildOperation {
                topic: topic.into(),
            },
        )?;
        let request_payload: ResponsePayload = serde_json::from_str(message.payload_str()?)?;

        Ok(Self {
            child_id,
            payload: request_payload,
        })
    }
}

pub struct DownloadFirmwareStatusMessage {}

impl TryIntoOperationStatusMessage for DownloadFirmwareStatusMessage {
    fn status_executing() -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToExecuting::new(CumulocitySupportedOperations::C8yFirmware)
            .to_smartrest()
    }

    fn status_successful(
        _parameter: Option<String>,
    ) -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToSuccessful::new(CumulocitySupportedOperations::C8yFirmware)
            .to_smartrest()
    }

    fn status_failed(failure_reason: String) -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToFailed::new(
            CumulocitySupportedOperations::C8yFirmware,
            failure_reason,
        )
        .to_smartrest()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use assert_json_diff::assert_json_eq;
    use assert_matches::assert_matches;
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

        let message: MqttMessage = firmware_operation_request.try_into().unwrap();
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
            serde_json::from_str::<serde_json::Value>(message.payload_str().unwrap()).unwrap(),
            expected_json
        );
    }

    #[test]
    fn create_firmware_operation_response() {
        let incoming_payload = json!({
            "status": "executing",
            "id": "op-id",
            "reason": null
        })
        .to_string();
        let incoming_message = MqttMessage::new(
            &Topic::new_unchecked("tedge/child-id/commands/res/firmware_update"),
            incoming_payload,
        );
        let firmware_response = FirmwareOperationResponse::try_from(&incoming_message).unwrap();

        let expected_payload = ResponsePayload {
            operation_id: "op-id".to_string(),
            status: OperationStatus::Executing,
            reason: None,
        };

        assert_eq!(firmware_response.get_payload(), &expected_payload);
        assert_eq!(firmware_response.get_child_id(), "child-id");
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
        let incoming_payload = json!({
            "status": "failed",
            "id": "op-id",
            "reason": "aaa"
        })
        .to_string();
        let message = MqttMessage::new(
            &Topic::new_unchecked("tedge/child-id/commands/res/firmware_update"),
            incoming_payload,
        );
        let firmware_response = FirmwareOperationResponse::try_from(&message).unwrap();
        let expected_response_payload = ResponsePayload {
            operation_id: "op-id".to_string(),
            status: OperationStatus::Failed,
            reason: Some("aaa".to_string()),
        };
        assert_eq!(firmware_response.payload, expected_response_payload);
    }

    #[test]
    fn deserialize_response_payload_with_only_operation_id() {
        let incoming_payload = json!({
            "id": "op-id",
        })
        .to_string();
        let message = MqttMessage::new(
            &Topic::new_unchecked("tedge/child-id/commands/res/firmware_update"),
            incoming_payload,
        );
        let result = FirmwareOperationResponse::try_from(&message);
        assert_matches!(
            result.unwrap_err(),
            FirmwareManagementError::FromSerdeJsonError { .. }
        );
    }

    #[test]
    fn deserialize_response_payload_with_invalid_operation_status() {
        let incoming_payload = json!({
            "status": "invalid",
            "id": "op-id",
        })
        .to_string();
        let message = MqttMessage::new(
            &Topic::new_unchecked("tedge/child-id/commands/res/firmware_update"),
            incoming_payload,
        );
        let result = FirmwareOperationResponse::try_from(&message);
        assert_matches!(
            result.unwrap_err(),
            FirmwareManagementError::FromSerdeJsonError { .. }
        );
    }

    #[test]
    fn deserialize_response_payload_with_invalid_reason() {
        let incoming_payload = json!({
            "reason": 00,
            "id": "op-id",
        })
        .to_string();
        let message = MqttMessage::new(
            &Topic::new_unchecked("tedge/child-id/commands/res/firmware_update"),
            incoming_payload,
        );
        let result = FirmwareOperationResponse::try_from(&message);
        assert_matches!(
            result.unwrap_err(),
            FirmwareManagementError::FromSerdeJsonError { .. }
        );
    }

    #[test]
    fn deserialize_response_payload_without_operation_id() {
        let incoming_payload = json!({
            "status": "executing",
            "reason": "aaa"
        })
        .to_string();
        let message = MqttMessage::new(
            &Topic::new_unchecked("tedge/child-id/commands/res/firmware_update"),
            incoming_payload,
        );
        let result = FirmwareOperationResponse::try_from(&message);
        assert_matches!(
            result.unwrap_err(),
            FirmwareManagementError::FromSerdeJsonError { .. }
        );
    }
}
