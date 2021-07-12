use crate::software::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SoftwareRequest {
    pub id: String,

    #[serde(flatten)]
    pub operation: SoftwareOperation,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SoftwareResponse {
    pub id: String,

    #[serde(flatten)]
    pub status: SoftwareOperationStatus,
}

impl SoftwareRequest {
    pub fn from_json(json_str: &str) -> Result<SoftwareRequest, SoftwareError> {
        Ok(serde_json::from_str(json_str)?)
    }

    pub fn to_json(&self) -> Result<String, SoftwareError> {
        Ok(serde_json::to_string(self)?)
    }
}

impl SoftwareResponse {
    pub fn from_json(json_str: &str) -> Result<SoftwareResponse, SoftwareError> {
        Ok(serde_json::from_str(json_str)?)
    }

    pub fn to_json(&self) -> Result<String, SoftwareError> {
        Ok(serde_json::to_string(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_and_parse_software_updates() {
        let request = SoftwareRequest {
            id: String::from("42"),
            operation: SoftwareOperation::SoftwareUpdates {
                updates: vec![
                    SoftwareUpdate::Install {
                        module: SoftwareModule {
                            software_type: String::from("default"),
                            name: String::from("collectd-core"),
                            version: None,
                            url: None,
                        },
                    },
                    SoftwareUpdate::Install {
                        module: SoftwareModule {
                            software_type: String::from("debian"),
                            name: String::from("ripgrep"),
                            version: None,
                            url: None,
                        },
                    },
                    SoftwareUpdate::UnInstall {
                        module: SoftwareModule {
                            software_type: String::from("default"),
                            name: String::from("hexyl"),
                            version: None,
                            url: None,
                        },
                    },
                ],
            },
        };

        let expected_json = r#"{"id":"42","updates":[{"action":"install","type":"default","name":"collectd-core"},{"action":"install","type":"debian","name":"ripgrep"},{"action":"uninstall","type":"default","name":"hexyl"}]}"#;

        let actual_json = request.to_json().expect("Fail to serialize the request");
        assert_eq!(actual_json, expected_json);

        let parsed_request =
            SoftwareRequest::from_json(&actual_json).expect("Fail to parse the json request");
        assert_eq!(parsed_request, request);
    }

    #[test]
    fn serialize_and_parse_update_status() {
        let status = SoftwareUpdateStatus {
            update: SoftwareUpdate::Install {
                module: SoftwareModule {
                    software_type: String::from("default"),
                    name: String::from("collectd-core"),
                    version: None,
                    url: None,
                },
            },
            status: UpdateStatus::Success,
        };

        let expected_json = r#"{"update":{"action":"install","type":"default","name":"collectd-core"},"status":"Success"}"#;
        let actual_json = serde_json::to_string(&status).expect("Fail to serialize a status");
        assert_eq!(actual_json, expected_json);

        let parsed_status: SoftwareUpdateStatus =
            serde_json::from_str(&actual_json).expect("Fail to parse the json status");
        assert_eq!(parsed_status, status);
    }

    #[test]
    fn serialize_and_parse_software_list() {
        let request = SoftwareRequest {
            id: String::from("42"),
            operation: SoftwareOperation::CurrentSoftwareList { list: () },
        };
        let expected_json = r#"{"id":"42","list":null}"#;

        let actual_json = request.to_json().expect("Fail to serialize the request");
        assert_eq!(actual_json, expected_json);

        let parsed_request =
            SoftwareRequest::from_json(&actual_json).expect("Fail to parse the json request");
        assert_eq!(parsed_request, request);
    }
}
