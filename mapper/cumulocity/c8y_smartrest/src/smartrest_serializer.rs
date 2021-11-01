use crate::error::SmartRestSerializerError;
use csv::{QuoteStyle, WriterBuilder};
use json_sm::{SoftwareOperationStatus, SoftwareUpdateResponse};
use serde::{Deserialize, Serialize, Serializer};

type SmartRest = String;

#[derive(Debug)]
pub enum CumulocitySupportedOperations {
    C8ySoftwareUpdate,
    C8yLogFileRequest,
}

impl From<CumulocitySupportedOperations> for &'static str {
    fn from(op: CumulocitySupportedOperations) -> Self {
        match op {
            CumulocitySupportedOperations::C8ySoftwareUpdate => "c8y_SoftwareUpdate",
            CumulocitySupportedOperations::C8yLogFileRequest => "c8y_LogfileRequest",
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
    pub supported_operations: Vec<&'static str>,
}

impl Default for SmartRestSetSupportedLogType {
    fn default() -> Self {
        Self {
            message_id: "118",
            supported_operations: vec!["software-management".into()],
        }
    }
}

impl<'a> SmartRestSerializer<'a> for SmartRestSetSupportedLogType {}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SmartRestSetSupportedOperations {
    pub message_id: &'static str,
    pub supported_operations: Vec<&'static str>,
}

impl Default for SmartRestSetSupportedOperations {
    fn default() -> Self {
        Self {
            message_id: "114",
            supported_operations: vec![
                CumulocitySupportedOperations::C8ySoftwareUpdate.into(),
                CumulocitySupportedOperations::C8yLogFileRequest.into(),
            ],
        }
    }
}

impl<'a> SmartRestSerializer<'a> for SmartRestSetSupportedOperations {}

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
            SoftwareOperationStatus::Executing => {
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
    pub url: Option<String>,
}

impl SmartRestSetOperationToSuccessful {
    fn new(operation: CumulocitySupportedOperations) -> Self {
        Self {
            message_id: "503",
            operation: operation.into(),
            url: None,
        }
    }

    pub fn new_with_file(operation: CumulocitySupportedOperations, url: &str) -> Self {
        Self {
            message_id: "503",
            operation: operation.into(),
            url: Some(url.into()),
        }
    }

    pub fn from_thin_edge_json(
        response: SoftwareUpdateResponse,
    ) -> Result<Self, SmartRestSerializerError> {
        match response.status() {
            SoftwareOperationStatus::Successful => {
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
    fn new(operation: CumulocitySupportedOperations, reason: String) -> Self {
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
            SoftwareOperationStatus::Failed => Ok(Self::new(
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
    let mut wtr = WriterBuilder::new()
        .has_headers(false)
        .quote_style(QuoteStyle::Never)
        .double_quote(false)
        .from_writer(vec![]);
    wtr.serialize(record)?;
    let csv = String::from_utf8(wtr.into_inner()?)?;
    Ok(csv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use json_sm::*;

    #[test]
    fn serialize_smartrest_supported_operations() {
        let smartrest = SmartRestSetSupportedOperations::default()
            .to_smartrest()
            .unwrap();
        assert_eq!(smartrest, "114,c8y_SoftwareUpdate\n");
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
        assert_eq!(smartrest, "503,c8y_SoftwareUpdate\n");
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
            url: None,
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
