use crate::error::SmartRestSerializerError;
use crate::topic::C8yTopic;
use agent_interface::{OperationStatus, SoftwareUpdateResponse};
use csv::{QuoteStyle, WriterBuilder};
use mqtt_channel::Message;
use serde::{Deserialize, Serialize, Serializer};

pub type SmartRest = String;

#[derive(Debug)]
pub enum CumulocitySupportedOperations {
    C8ySoftwareUpdate,
    C8yLogFileRequest,
    C8yRestartRequest,
    C8yUploadConfigFile,
    C8yDownloadConfigFile,
}

impl From<CumulocitySupportedOperations> for &'static str {
    fn from(op: CumulocitySupportedOperations) -> Self {
        match op {
            CumulocitySupportedOperations::C8ySoftwareUpdate => "c8y_SoftwareUpdate",
            CumulocitySupportedOperations::C8yLogFileRequest => "c8y_LogfileRequest",
            CumulocitySupportedOperations::C8yRestartRequest => "c8y_Restart",
            CumulocitySupportedOperations::C8yUploadConfigFile => "c8y_UploadConfigFile",
            CumulocitySupportedOperations::C8yDownloadConfigFile => "c8y_DownloadConfigFile",
        }
    }
}

pub trait SmartRestSerializer<'a>
where
    Self: Serialize,
{
    fn to_smartrest(&self) -> Result<SmartRest, SmartRestSerializerError> {
        serialize_smartrest(self)
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SmartRestSetSupportedLogType {
    pub message_id: &'static str,
    pub supported_operations: Vec<String>,
}

impl From<Vec<String>> for SmartRestSetSupportedLogType {
    fn from(operation_types: Vec<String>) -> Self {
        Self {
            message_id: "118",
            supported_operations: operation_types,
        }
    }
}

impl<'a> SmartRestSerializer<'a> for SmartRestSetSupportedLogType {}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SmartRestSetSupportedOperations<'a> {
    pub message_id: &'static str,
    pub supported_operations: Vec<&'a str>,
}

impl<'a> SmartRestSetSupportedOperations<'a> {
    pub fn new(supported_operations: &[&'a str]) -> Self {
        Self {
            message_id: "114",
            supported_operations: supported_operations.into(),
        }
    }

    pub fn add_operation(&mut self, operation: &'a str) {
        self.supported_operations.push(operation);
    }
}

impl<'a> SmartRestSerializer<'a> for SmartRestSetSupportedOperations<'a> {}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SmartRestSoftwareModuleItem {
    pub software: String,
    pub version: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SmartRestGetPendingOperations {
    pub id: &'static str,
}

impl Default for SmartRestGetPendingOperations {
    fn default() -> Self {
        Self { id: "500" }
    }
}

impl<'a> SmartRestSerializer<'a> for SmartRestGetPendingOperations {}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SmartRestSetOperationToExecuting {
    pub message_id: &'static str,
    pub operation: &'static str,
}

impl SmartRestSetOperationToExecuting {
    pub fn new(operation: CumulocitySupportedOperations) -> Self {
        Self {
            message_id: "501",
            operation: operation.into(),
        }
    }

    pub fn from_thin_edge_json(
        response: SoftwareUpdateResponse,
    ) -> Result<Self, SmartRestSerializerError> {
        match response.status() {
            OperationStatus::Executing => {
                Ok(Self::new(CumulocitySupportedOperations::C8ySoftwareUpdate))
            }
            _ => Err(SmartRestSerializerError::UnsupportedOperationStatus { response }),
        }
    }
}

impl<'a> SmartRestSerializer<'a> for SmartRestSetOperationToExecuting {}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SmartRestSetOperationToSuccessful {
    pub message_id: &'static str,
    pub operation: &'static str,
    pub operation_parameter: Option<String>,
}

impl SmartRestSetOperationToSuccessful {
    pub fn new(operation: CumulocitySupportedOperations) -> Self {
        Self {
            message_id: "503",
            operation: operation.into(),
            operation_parameter: None,
        }
    }

    pub fn with_response_parameter(self, response_parameter: &str) -> Self {
        Self {
            operation_parameter: Some(response_parameter.into()),
            ..self
        }
    }

    pub fn from_thin_edge_json(
        response: SoftwareUpdateResponse,
    ) -> Result<Self, SmartRestSerializerError> {
        match response.status() {
            OperationStatus::Successful => {
                Ok(Self::new(CumulocitySupportedOperations::C8ySoftwareUpdate))
            }
            _ => Err(SmartRestSerializerError::UnsupportedOperationStatus { response }),
        }
    }
}

impl<'a> SmartRestSerializer<'a> for SmartRestSetOperationToSuccessful {}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SmartRestSetOperationToFailed {
    pub message_id: &'static str,
    pub operation: &'static str,
    #[serde(serialize_with = "reason_to_string_with_quotes")]
    pub reason: String,
}

impl SmartRestSetOperationToFailed {
    pub fn new(operation: CumulocitySupportedOperations, reason: String) -> Self {
        Self {
            message_id: "502",
            operation: operation.into(),
            reason,
        }
    }

    pub fn from_thin_edge_json(
        response: SoftwareUpdateResponse,
    ) -> Result<Self, SmartRestSerializerError> {
        match &response.status() {
            OperationStatus::Failed => Ok(Self::new(
                CumulocitySupportedOperations::C8ySoftwareUpdate,
                response.error().unwrap_or_else(|| "".to_string()),
            )),
            _ => Err(SmartRestSerializerError::UnsupportedOperationStatus { response }),
        }
    }
}

impl<'a> SmartRestSerializer<'a> for SmartRestSetOperationToFailed {}

fn reason_to_string_with_quotes<S>(reason: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let s = format!("\"{}\"", reason);
    serializer.serialize_str(&s)
}

fn serialize_smartrest<S: Serialize>(record: S) -> Result<String, SmartRestSerializerError> {
    let mut wtr = Box::new(
        WriterBuilder::new()
            .has_headers(false)
            .quote_style(QuoteStyle::Never)
            .double_quote(false)
            .from_writer(vec![]),
    );
    wtr.serialize(record)?;
    let csv = String::from_utf8(wtr.into_inner()?)?;
    Ok(csv)
}

/// Helper to generate a SmartREST operation status message
pub trait TryIntoOperationStatusMessage {
    fn executing() -> Result<Message, SmartRestSerializerError> {
        let status = Self::status_executing()?;
        Ok(Self::create_message(status))
    }

    fn successful(parameter: Option<String>) -> Result<Message, SmartRestSerializerError> {
        let status = Self::status_successful(parameter)?;
        Ok(Self::create_message(status))
    }

    fn failed(failure_reason: String) -> Result<Message, SmartRestSerializerError> {
        let status = Self::status_failed(failure_reason)?;
        Ok(Self::create_message(status))
    }

    fn create_message(payload: SmartRest) -> Message {
        let topic = C8yTopic::SmartRestResponse.to_topic().unwrap(); // never fail
        Message::new(&topic, payload)
    }

    fn status_executing() -> Result<SmartRest, SmartRestSerializerError>;
    fn status_successful(parameter: Option<String>) -> Result<SmartRest, SmartRestSerializerError>;
    fn status_failed(failure_reason: String) -> Result<SmartRest, SmartRestSerializerError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_interface::*;

    #[test]
    fn serialize_smartrest_supported_operations() {
        let smartrest =
            SmartRestSetSupportedOperations::new(&["c8y_SoftwareUpdate", "c8y_LogfileRequest"])
                .to_smartrest()
                .unwrap();
        assert_eq!(smartrest, "114,c8y_SoftwareUpdate,c8y_LogfileRequest\n");
    }

    #[test]
    fn serialize_smartrest_get_pending_operations() {
        let smartrest = SmartRestGetPendingOperations::default()
            .to_smartrest()
            .unwrap();
        assert_eq!(smartrest, "500\n");
    }

    #[test]
    fn serialize_smartrest_set_operation_to_executing() {
        let smartrest =
            SmartRestSetOperationToExecuting::new(CumulocitySupportedOperations::C8ySoftwareUpdate)
                .to_smartrest()
                .unwrap();
        assert_eq!(smartrest, "501,c8y_SoftwareUpdate\n");
    }

    #[test]
    fn from_thin_edge_json_to_smartrest_set_operation_to_executing() {
        let json_response = r#"{
            "id": "123",
            "status": "executing"
        }"#;
        let response = SoftwareUpdateResponse::from_json(json_response).unwrap();
        let smartrest_obj =
            SmartRestSetOperationToExecuting::from_thin_edge_json(response).unwrap();

        let expected_smartrest_obj = SmartRestSetOperationToExecuting {
            message_id: "501",
            operation: "c8y_SoftwareUpdate",
        };
        assert_eq!(smartrest_obj, expected_smartrest_obj);
    }

    #[test]
    fn serialize_smartrest_set_operation_to_successful() {
        let smartrest = SmartRestSetOperationToSuccessful::new(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
        )
        .to_smartrest()
        .unwrap();
        assert_eq!(smartrest, "503,c8y_SoftwareUpdate,\n");
    }

    #[test]
    fn from_thin_edge_json_to_smartrest_set_operation_to_successful() {
        let json_response = r#"{
            "id":"1",
            "status":"successful",
            "currentSoftwareList":[]
            }"#;
        let response = SoftwareUpdateResponse::from_json(json_response).unwrap();
        let smartrest_obj =
            SmartRestSetOperationToSuccessful::from_thin_edge_json(response).unwrap();

        let expected_smartrest_obj = SmartRestSetOperationToSuccessful {
            message_id: "503",
            operation: "c8y_SoftwareUpdate",
            operation_parameter: None,
        };
        assert_eq!(smartrest_obj, expected_smartrest_obj);
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed() {
        let smartrest = SmartRestSetOperationToFailed::new(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            "Failed due to permission.".into(),
        )
        .to_smartrest()
        .unwrap();
        assert_eq!(
            smartrest,
            "502,c8y_SoftwareUpdate,\"Failed due to permission.\"\n"
        );
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed_with_comma_reason() {
        let smartrest = SmartRestSetOperationToFailed::new(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            "Failed to install collectd, modbus, and golang.".into(),
        )
        .to_smartrest()
        .unwrap();
        assert_eq!(
            smartrest,
            "502,c8y_SoftwareUpdate,\"Failed to install collectd, modbus, and golang.\"\n"
        );
    }

    #[test]
    fn serialize_smartrest_set_operation_to_failed_with_empty_reason() {
        let smartrest = SmartRestSetOperationToFailed::new(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            "".into(),
        )
        .to_smartrest()
        .unwrap();
        assert_eq!(smartrest, "502,c8y_SoftwareUpdate,\"\"\n");
    }

    #[test]
    fn from_thin_edge_json_to_smartrest_set_operation_to_failed() {
        let json_response = r#"{
            "id": "123",
            "status":"failed",
            "reason":"2 errors: fail to install [ collectd ] fail to remove [ mongodb ]",
            "currentSoftwareList": [],
            "failures": []
        }"#;
        let response = SoftwareUpdateResponse::from_json(json_response).unwrap();

        let smartrest_obj = SmartRestSetOperationToFailed::new(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            response.error().unwrap(),
        );

        let expected_smartrest_obj = SmartRestSetOperationToFailed {
            message_id: "502",
            operation: "c8y_SoftwareUpdate",
            reason: "2 errors: fail to install [ collectd ] fail to remove [ mongodb ]".to_string(),
        };
        assert_eq!(smartrest_obj, expected_smartrest_obj);
    }

    #[test]
    fn from_thin_edge_json_to_smartrest_set_operation_to_failed_with_empty_reason() {
        let json_response = r#"{
            "id": "123",
            "status":"failed",
            "reason":"",
            "currentSoftwareList": [],
            "failures": []
        }"#;
        let response = SoftwareUpdateResponse::from_json(json_response).unwrap();

        let smartrest_obj = SmartRestSetOperationToFailed::new(
            CumulocitySupportedOperations::C8ySoftwareUpdate,
            response.error().unwrap(),
        );

        let expected_smartrest_obj = SmartRestSetOperationToFailed {
            message_id: "502",
            operation: "c8y_SoftwareUpdate",
            reason: "".to_string(),
        };
        assert_eq!(smartrest_obj, expected_smartrest_obj);
    }
}
