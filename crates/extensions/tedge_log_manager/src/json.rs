use crate::error::LogManagementError;
use serde::Deserialize;
use serde::Serialize;
use std::convert::TryFrom;
use tedge_mqtt_ext::MqttMessage;
use time::OffsetDateTime;

// This Enum will be reverted once tedge mapper crate will be merged
#[derive(Debug, Deserialize, Serialize, PartialEq, Copy, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub enum CommandStatus {
    Init,
    Executing,
    Successful,
    Failed,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogInfo {
    #[serde(rename = "type")]
    pub log_type: String,
    #[serde(with = "time::serde::rfc3339")]
    pub date_from: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub date_to: OffsetDateTime,
    pub lines: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_text: Option<String>,
}

impl LogInfo {
    pub fn new(
        log_type: &str,
        date_from: OffsetDateTime,
        date_to: OffsetDateTime,
        lines: usize,
    ) -> Self {
        Self {
            log_type: log_type.to_string(),
            date_from,
            date_to,
            search_text: None,
            lines,
        }
    }

    pub fn with_search_text(self, needle: &str) -> Self {
        Self {
            search_text: Some(needle.into()),
            ..self
        }
    }
}

impl ToString for LogInfo {
    fn to_string(&self) -> String {
        serde_json::to_string(&self).expect("infallible")
    }
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogRequestPayload {
    pub status: CommandStatus,
    pub tedge_url: String,
    #[serde(flatten)]
    pub log: LogInfo,
}

impl TryFrom<MqttMessage> for LogRequestPayload {
    type Error = LogManagementError;

    fn try_from(value: MqttMessage) -> Result<Self, Self::Error> {
        let payload = value.payload.as_str()?;
        let request: LogRequestPayload = serde_json::from_str(payload)?;
        Ok(request)
    }
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LogResponsePayload {
    pub status: CommandStatus,
    pub tedge_url: String,
    #[serde(flatten)]
    pub log: LogInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl LogResponsePayload {
    pub fn from_log_request(request: &LogRequestPayload, status: CommandStatus) -> Self {
        Self {
            status,
            tedge_url: request.tedge_url.clone(),
            log: request.log.clone(),
            reason: None,
        }
    }

    pub fn with_reason(self, reason: &str) -> Self {
        Self {
            reason: Some(reason.into()),
            ..self
        }
    }
}

impl ToString for LogResponsePayload {
    fn to_string(&self) -> String {
        serde_json::to_string(&self).expect("infallible")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_json_diff::*;
    use serde_json::json;
    use time::macros::datetime;

    #[test]
    fn serialize_response_payload() {
        let log = LogInfo::new(
            "type_one",
            datetime!(1970-01-01 00:00:00 +00:00),
            datetime!(1970-01-01 00:00:03 +00:00),
            7,
        );
        let request_payload = LogRequestPayload {
            status: CommandStatus::Init,
            tedge_url: "http://127.0.0.1:3000/tedge/file-transfer/main/logfile/type_one-opid"
                .to_string(),
            log,
        };

        let response_payload =
            LogResponsePayload::from_log_request(&request_payload, CommandStatus::Executing);

        let json = serde_json::to_string(&response_payload).unwrap();

        let expected_json = json!({
            "status": "executing",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/main/logfile/type_one-opid",
            "type": "type_one",
            "dateFrom": "1970-01-01T00:00:00Z",
            "dateTo": "1970-01-01T00:00:03Z",
            "lines": 7
        });

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&json).unwrap(),
            expected_json
        );
    }

    #[test]
    fn serialize_response_payload_with_reason() {
        let log = LogInfo::new(
            "type_one",
            datetime!(1970-01-01 00:00:00 +00:00),
            datetime!(1970-01-01 00:00:03 +00:00),
            7,
        );
        let request_payload = LogRequestPayload {
            status: CommandStatus::Init,
            tedge_url: "http://127.0.0.1:3000/tedge/file-transfer/main/logfile/type_one-opid"
                .to_string(),
            log,
        };

        let response_payload =
            LogResponsePayload::from_log_request(&request_payload, CommandStatus::Executing)
                .with_reason("something");

        let json = serde_json::to_string(&response_payload).unwrap();

        let expected_json = json!({
            "status": "executing",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/main/logfile/type_one-opid",
            "type": "type_one",
            "dateFrom": "1970-01-01T00:00:00Z",
            "dateTo": "1970-01-01T00:00:03Z",
            "lines": 7,
            "reason": "something"
        });

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&json).unwrap(),
            expected_json
        );
    }

    #[test]
    fn deserialize_log_request() {
        let data = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_one-1234",
            "type": "type_one",
            "dateFrom": "1970-01-01T00:00:00+00:00",
            "dateTo": "1970-01-01T00:00:03+00:00",
            "lines": 7
        }"#;
        let value: LogRequestPayload = serde_json::from_str(data).unwrap();

        let expected_value = LogRequestPayload {
            status: CommandStatus::Init,
            tedge_url: "http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_one-1234"
                .to_string(),
            log: LogInfo {
                log_type: "type_one".to_string(),
                date_from: datetime!(1970-01-01 00:00:00 +00:00),
                date_to: datetime!(1970-01-01 00:00:03 +00:00),
                lines: 7,
                search_text: None,
            },
        };

        assert_eq!(value, expected_value);
    }

    #[test]
    fn deserialize_log_request_with_search_text() {
        let data = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_one-1234",
            "type": "type_one",
            "dateFrom": "1970-01-01T00:00:00+00:00",
            "dateTo": "1970-01-01T00:00:03+00:00",
            "lines": 7,
            "searchText": "needle"
        }"#;
        let value: LogRequestPayload = serde_json::from_str(data).unwrap();

        let expected_value = LogRequestPayload {
            status: CommandStatus::Init,
            tedge_url: "http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_one-1234"
                .to_string(),
            log: LogInfo {
                log_type: "type_one".to_string(),
                date_from: datetime!(1970-01-01 00:00:00 +00:00),
                date_to: datetime!(1970-01-01 00:00:03 +00:00),
                lines: 7,
                search_text: Some("needle".to_string()),
            },
        };

        assert_eq!(value, expected_value);
    }
}
