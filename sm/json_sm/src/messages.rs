use crate::{error::SoftwareError, software::*};
use serde::{Deserialize, Serialize};

/// All the messages are serialized using json.
pub trait Jsonify<'a>
    where
        Self: Deserialize<'a> + Serialize + Sized,
{
    fn from_json(json_str: &'a str) -> Result<Self, SoftwareError> {
        Ok(serde_json::from_str(json_str)?)
    }

    fn from_slice(bytes: &'a [u8]) -> Result<Self, SoftwareError> {
        Ok(serde_json::from_slice(bytes)?)
    }

    fn to_json(&self) -> Result<String, SoftwareError> {
        Ok(serde_json::to_string(self)?)
    }

    fn to_bytes(&self) -> Result<Vec<u8>, SoftwareError> {
        Ok(serde_json::to_vec(self)?)
    }
}

/// Message payload definition for SoftwareList request.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareListRequest {
    pub id: usize,
}

impl<'a> Jsonify<'a> for SoftwareListRequest {}

impl SoftwareListRequest {
    pub fn new(id: usize) -> SoftwareListRequest {
        SoftwareListRequest { id }
    }
}

/// Message payload definition for SoftwareUpdate request.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareUpdateRequest {
    pub id: usize,
    pub update_list: Vec<SoftwareRequestResponseSoftwareList>,
}

impl<'a> Jsonify<'a> for SoftwareUpdateRequest {}

/// Sub list of modules grouped by plugin type.
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SoftwareRequestResponseSoftwareList {
    #[serde(rename = "type")]
    pub plugin_type: SoftwareType,
    pub list: Vec<SoftwareModuleItem>,
}

/// Possible statuses for result of Software operation.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SoftwareOperationStatus {
    Successful,
    Failed,
    Executing,
}

/// Message payload definition for SoftwareList response.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SoftwareListResponse {
    #[serde(flatten)]
    response: SoftwareRequestResponse,
}

impl<'a> Jsonify<'a> for SoftwareListResponse {}

impl SoftwareListResponse {
    pub fn new(req: &SoftwareListRequest) -> SoftwareListResponse {
        let response = SoftwareRequestResponse {
            id: req.id,
            status: SoftwareOperationStatus::Successful,
            reason: None,
            current_software_list: vec![],
            failures: vec![],
        };

        SoftwareListResponse {
            response
        }
    }

    pub fn add_modules(&mut self, plugin_type: &str, modules: Vec<SoftwareModule>) {
        self.response.current_software_list.push(SoftwareRequestResponseSoftwareList {
            plugin_type: plugin_type.to_string(),
            list: modules.into_iter().map(|module| module.into()).collect::<Vec<SoftwareModuleItem>>(),
        })
    }
}

/// Variants represent Software Operations Supported actions.
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SoftwareModuleAction {
    Install,
    Remove,
}

/// Software module payload definition.
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SoftwareModuleItem {
    pub name: SoftwareName,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<SoftwareVersion>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<SoftwareModuleAction>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Software Operation Response payload format.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareRequestResponse {
    // TODO: Is this the right approach, maybe nanoid?
    pub id: usize,
    pub status: SoftwareOperationStatus,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    #[serde(default)]
    pub current_software_list: Vec<SoftwareRequestResponseSoftwareList>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<SoftwareRequestResponseSoftwareList>,
}

impl<'a> Jsonify<'a> for SoftwareRequestResponse {}

// TODO: Add methods to handle response changes, eg add_failure, update reason ...
impl SoftwareRequestResponse {
    pub fn new(id: usize, status: SoftwareOperationStatus) -> Self {
        SoftwareRequestResponse {
            id,
            status,
            current_software_list: vec![],
            reason: None,
            failures: vec![],
        }
    }

    pub fn finalize_response(&mut self, software_list: Vec<SoftwareRequestResponseSoftwareList>) {
        if self.failures.is_empty() {
            self.status = SoftwareOperationStatus::Successful;
        }

        self.current_software_list = software_list;
    }
}

impl From<SoftwareModuleItem> for SoftwareModule {
    fn from(val: SoftwareModuleItem) -> Self {
        SoftwareModule {
            name: val.name,
            version: val.version,
            url: val.url,
        }
    }
}

impl From<SoftwareModuleItem> for Option<SoftwareModuleUpdate> {
    fn from(val: SoftwareModuleItem) -> Self {
        match val.action {
            Some(SoftwareModuleAction::Install) => {
                Some(SoftwareModuleUpdate::Install { module: val.into() })
            }
            Some(SoftwareModuleAction::Remove) => {
                Some(SoftwareModuleUpdate::Remove { module: val.into() })
            }
            None => None,
        }
    }
}

impl From<SoftwareModule> for SoftwareModuleItem {
    fn from(module: SoftwareModule) -> Self {
        SoftwareModuleItem {
            name: module.name,
            version: module.version,
            url: module.url,
            action: None,
            reason: None,
        }
    }
}

impl From<SoftwareModuleUpdate> for SoftwareModuleItem {
    fn from(update: SoftwareModuleUpdate) -> Self {
        match update {
            SoftwareModuleUpdate::Install { module } => SoftwareModuleItem {
                name: module.name,
                version: module.version,
                url: module.url,
                action: Some(SoftwareModuleAction::Install),
                reason: None,
            },
            SoftwareModuleUpdate::Remove { module } => SoftwareModuleItem {
                name: module.name,
                version: module.version,
                url: module.url,
                action: Some(SoftwareModuleAction::Remove),
                reason: None,
            },
        }
    }
}

impl From<SoftwareModuleUpdateResult> for SoftwareModuleItem {
    fn from(result: SoftwareModuleUpdateResult) -> Self {
        let mut msg: SoftwareModuleItem = result.update.into();
        msg.reason = result.error.map(|err| format!("{}", err));
        msg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_software_request_list() {
        let request = SoftwareListRequest { id: 1234 };
        let expected_json = r#"{"id":1234}"#;

        let actual_json = request.to_json().expect("Failed to serialize");

        assert_eq!(actual_json, expected_json);

        let de_request =
            SoftwareListRequest::from_json(actual_json.as_str()).expect("failed to deserialize");
        assert_eq!(request, de_request);
    }

    #[test]
    fn serde_software_request_update() {
        let debian_module1 = SoftwareModuleItem {
            name: "debian1".into(),
            version: Some("0.0.1".into()),
            action: Some(SoftwareModuleAction::Install),
            url: None,
            reason: None,
        };

        let debian_module2 = SoftwareModuleItem {
            name: "debian2".into(),
            version: Some("0.0.2".into()),
            action: Some(SoftwareModuleAction::Install),
            url: None,
            reason: None,
        };

        let debian_list = SoftwareRequestResponseSoftwareList {
            plugin_type: "debian".into(),
            list: vec![debian_module1, debian_module2],
        };

        let docker_module1 = SoftwareModuleItem {
            name: "docker1".into(),
            version: Some("0.0.1".into()),
            action: Some(SoftwareModuleAction::Remove),
            url: Some("test.com".into()),
            reason: None,
        };

        let docker_list = SoftwareRequestResponseSoftwareList {
            plugin_type: "docker".into(),
            list: vec![docker_module1],
        };

        let request = SoftwareUpdateRequest {
            id: 1234,
            update_list: vec![debian_list, docker_list],
        };

        let expected_json = r#"{"id":1234,"updateList":[{"type":"debian","list":[{"name":"debian1","version":"0.0.1","action":"install"},{"name":"debian2","version":"0.0.2","action":"install"}]},{"type":"docker","list":[{"name":"docker1","version":"0.0.1","action":"remove","url":"test.com"}]}]}"#;

        let actual_json = request.to_json().expect("Fail to serialize the request");
        assert_eq!(actual_json, expected_json);

        let parsed_request =
            SoftwareUpdateRequest::from_json(&actual_json).expect("Fail to parse the json request");
        assert_eq!(parsed_request, request);
    }

    #[test]
    fn serde_software_list_empty_successful() {
        let request = SoftwareRequestResponse {
            id: 1234,
            status: SoftwareOperationStatus::Successful,
            reason: None,
            current_software_list: vec![],
            failures: vec![],
        };

        let expected_json = r#"{"id":1234,"status":"successful","currentSoftwareList":[]}"#;

        let actual_json = request.to_json().expect("Fail to serialize the request");
        assert_eq!(actual_json, expected_json);

        let parsed_request = SoftwareRequestResponse::from_json(&actual_json)
            .expect("Fail to parse the json request");
        assert_eq!(parsed_request, request);
    }

    #[test]
    fn serde_software_list_some_modules_successful() {
        let module1 = SoftwareModuleItem {
            name: "debian1".into(),
            version: Some("0.0.1".into()),
            action: None,
            url: None,
            reason: None,
        };

        let docker_module1 = SoftwareRequestResponseSoftwareList {
            plugin_type: "debian".into(),
            list: vec![module1],
        };

        let request = SoftwareRequestResponse {
            id: 1234,
            status: SoftwareOperationStatus::Successful,
            reason: None,
            current_software_list: vec![docker_module1],
            failures: vec![],
        };

        let expected_json = r#"{"id":1234,"status":"successful","currentSoftwareList":[{"type":"debian","list":[{"name":"debian1","version":"0.0.1"}]}]}"#;

        let actual_json = request.to_json().expect("Fail to serialize the request");
        assert_eq!(actual_json, expected_json);

        let parsed_request = SoftwareRequestResponse::from_json(&actual_json)
            .expect("Fail to parse the json request");
        assert_eq!(parsed_request, request);
    }
}
