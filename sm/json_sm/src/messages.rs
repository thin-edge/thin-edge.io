use crate::{error::SoftwareError, software::*};
use nanoid::nanoid;
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

pub const fn software_filter_topic() -> &'static str {
    "tedge/commands/req/software/#"
}

/// Message payload definition for SoftwareList request.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareListRequest {
    pub id: String,
}

impl<'a> Jsonify<'a> for SoftwareListRequest {}

impl SoftwareListRequest {
    pub fn new() -> SoftwareListRequest {
        let id = nanoid!();
        SoftwareListRequest { id }
    }

    pub fn new_with_id(id: &str) -> SoftwareListRequest {
        SoftwareListRequest { id: id.to_string() }
    }

    pub fn topic_name() -> &'static str {
        "tedge/commands/req/software/list"
    }
}

/// Message payload definition for SoftwareUpdate request.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareUpdateRequest {
    pub id: String,
    pub update_list: Vec<SoftwareRequestResponseSoftwareList>,
}

impl<'a> Jsonify<'a> for SoftwareUpdateRequest {}

impl SoftwareUpdateRequest {
    pub fn new() -> SoftwareUpdateRequest {
        let id = nanoid!();
        SoftwareUpdateRequest {
            id,
            update_list: vec![],
        }
    }

    pub fn new_with_id(id: &str) -> SoftwareUpdateRequest {
        SoftwareUpdateRequest {
            id: id.to_string(),
            update_list: vec![],
        }
    }

    pub fn topic_name() -> &'static str {
        "tedge/commands/req/software/update"
    }

    pub fn add_update(&mut self, mut update: SoftwareModuleUpdate) {
        update.normalize();
        let plugin_type = update
            .module()
            .module_type
            .clone()
            .unwrap_or_else(SoftwareModule::default_type);

        if let Some(list) = self
            .update_list
            .iter_mut()
            .find(|list| list.plugin_type == plugin_type)
        {
            list.modules.push(update.into());
        } else {
            self.update_list.push(SoftwareRequestResponseSoftwareList {
                plugin_type,
                modules: vec![update.into()],
            });
        }
    }

    pub fn add_updates(&mut self, plugin_type: &str, updates: Vec<SoftwareModuleUpdate>) {
        self.update_list.push(SoftwareRequestResponseSoftwareList {
            plugin_type: plugin_type.to_string(),
            modules: updates
                .into_iter()
                .map(|update| update.into())
                .collect::<Vec<SoftwareModuleItem>>(),
        })
    }

    pub fn modules_types(&self) -> Vec<SoftwareType> {
        let mut modules_types = vec![];

        for updates_per_type in self.update_list.iter() {
            modules_types.push(updates_per_type.plugin_type.clone())
        }

        modules_types
    }

    pub fn updates_for(&self, module_type: &str) -> Vec<SoftwareModuleUpdate> {
        let mut updates = vec![];

        if let Some(items) = self
            .update_list
            .iter()
            .find(|&items| items.plugin_type == module_type)
        {
            for item in items.modules.iter() {
                let module = SoftwareModule {
                    module_type: Some(module_type.to_string()),
                    name: item.name.clone(),
                    version: item.version.clone(),
                    url: item.url.clone(),
                    file_path: None,
                };
                match item.action {
                    None => {}
                    Some(SoftwareModuleAction::Install) => {
                        updates.push(SoftwareModuleUpdate::install(module));
                    }
                    Some(SoftwareModuleAction::Remove) => {
                        updates.push(SoftwareModuleUpdate::remove(module));
                    }
                }
            }
        }

        updates
    }
}

/// Sub list of modules grouped by plugin type.
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SoftwareRequestResponseSoftwareList {
    #[serde(rename = "type")]
    pub plugin_type: SoftwareType,
    pub modules: Vec<SoftwareModuleItem>,
}

/// Possible statuses for result of Software operation.
#[derive(Debug, Deserialize, Serialize, PartialEq, Copy, Clone)]
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
        SoftwareListResponse {
            response: SoftwareRequestResponse::new(&req.id, SoftwareOperationStatus::Executing),
        }
    }

    pub fn topic_name() -> &'static str {
        "tedge/commands/res/software/list"
    }

    pub fn add_modules(&mut self, plugin_type: &str, modules: Vec<SoftwareModule>) {
        self.response.add_modules(
            plugin_type.to_string(),
            modules
                .into_iter()
                .map(|module| module.into())
                .collect::<Vec<SoftwareModuleItem>>(),
        );
    }

    pub fn set_error(&mut self, reason: &str) {
        self.response.status = SoftwareOperationStatus::Failed;
        self.response.reason = Some(reason.into());
    }

    pub fn id(&self) -> &str {
        &self.response.id
    }

    pub fn status(&self) -> SoftwareOperationStatus {
        self.response.status
    }

    pub fn error(&self) -> Option<String> {
        self.response.reason.clone()
    }

    pub fn modules(&self) -> Vec<SoftwareModule> {
        self.response.modules()
    }
}

/// Message payload definition for SoftwareUpdate response.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SoftwareUpdateResponse {
    #[serde(flatten)]
    response: SoftwareRequestResponse,

    #[serde(skip)]
    errors: Vec<SoftwareError>,
}

impl<'a> Jsonify<'a> for SoftwareUpdateResponse {}

impl SoftwareUpdateResponse {
    pub fn new(req: &SoftwareUpdateRequest) -> SoftwareUpdateResponse {
        SoftwareUpdateResponse {
            response: SoftwareRequestResponse::new(&req.id, SoftwareOperationStatus::Executing),
            errors: vec![],
        }
    }

    pub fn topic_name() -> &'static str {
        "tedge/commands/res/software/update"
    }

    pub fn add_modules(&mut self, plugin_type: &str, modules: Vec<SoftwareModule>) {
        self.response.add_modules(
            plugin_type.to_string(),
            modules
                .into_iter()
                .map(|module| module.into())
                .collect::<Vec<SoftwareModuleItem>>(),
        );
    }

    pub fn add_errors(&mut self, plugin_type: &str, errors: Vec<SoftwareError>) {
        self.errors.append(&mut errors.clone());
        self.response.set_reason_errors(&self.errors);
        self.response.add_errors(
            plugin_type.to_string(),
            errors
                .into_iter()
                .filter_map(|module| module.into())
                .collect::<Vec<SoftwareModuleItem>>(),
        );
    }

    pub fn id(&self) -> &str {
        &self.response.id
    }

    pub fn status(&self) -> SoftwareOperationStatus {
        self.response.status
    }

    pub fn error(&self) -> Option<String> {
        self.response.reason.clone()
    }

    pub fn modules(&self) -> Vec<SoftwareModule> {
        self.response.modules()
    }
}

/// Variants represent Software Operations Supported actions.
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SoftwareModuleAction {
    Install,
    Remove,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct DownloadInfo {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
}

impl From<&str> for DownloadInfo {
    fn from(url: &str) -> Self {
        Self::new(url)
    }
}

impl DownloadInfo {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.into(),
            auth: None,
        }
    }

    pub fn with_auth(self, auth: Auth) -> Self {
        Self {
            auth: Some(auth),
            ..self
        }
    }

    pub fn url(&self) -> &str {
        self.url.as_str()
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub enum Auth {
    Bearer(String),
}

impl Auth {
    pub fn new_bearer(token: &str) -> Self {
        Self::Bearer(token.into())
    }
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
    #[serde(flatten)]
    pub url: Option<DownloadInfo>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<SoftwareModuleAction>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Software Operation Response payload format.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareRequestResponse {
    pub id: String,
    pub status: SoftwareOperationStatus,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_software_list: Option<Vec<SoftwareRequestResponseSoftwareList>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<SoftwareRequestResponseSoftwareList>,
}

impl<'a> Jsonify<'a> for SoftwareRequestResponse {}

impl SoftwareRequestResponse {
    pub fn new(id: &str, status: SoftwareOperationStatus) -> Self {
        SoftwareRequestResponse {
            id: id.to_string(),
            status,
            current_software_list: None,
            reason: None,
            failures: vec![],
        }
    }

    pub fn add_modules(&mut self, plugin_type: SoftwareType, modules: Vec<SoftwareModuleItem>) {
        if self.failures.is_empty() {
            self.status = SoftwareOperationStatus::Successful;
        }

        if self.current_software_list.is_none() {
            self.current_software_list = Some(vec![]);
        }

        if let Some(list) = self.current_software_list.as_mut() {
            list.push(SoftwareRequestResponseSoftwareList {
                plugin_type,
                modules,
            })
        }
    }

    pub fn set_reason_errors(&mut self, errors: &[SoftwareError]) {
        let mut count = 0;
        let mut failed_package = String::new();
        let mut failed_install = String::new();
        let mut failed_remove = String::new();

        for error in errors.iter() {
            count += 1;
            match error {
                SoftwareError::Install { module, .. } => {
                    failed_install.push(' ');
                    failed_install.push_str(&module.name);
                }
                SoftwareError::Remove { module, .. } => {
                    failed_remove.push(' ');
                    failed_remove.push_str(&module.name);
                }
                SoftwareError::Prepare { software_type, .. } => {
                    failed_package.push(' ');
                    failed_package.push_str(software_type);
                }
                SoftwareError::Finalize { software_type, .. } => {
                    failed_package.push(' ');
                    failed_package.push_str(software_type);
                }
                _ => {}
            }
        }

        let mut reason = format!("{} errors:", count);

        if !failed_package.is_empty() {
            reason.push_str(" fail to update [");
            reason.push_str(&failed_package);
            reason.push_str(" ]");
        }
        if !failed_install.is_empty() {
            reason.push_str(" fail to install [");
            reason.push_str(&failed_install);
            reason.push_str(" ]");
        }
        if !failed_remove.is_empty() {
            reason.push_str(" fail to remove [");
            reason.push_str(&failed_remove);
            reason.push_str(" ]");
        }

        self.status = SoftwareOperationStatus::Failed;
        self.reason = Some(reason);
    }

    pub fn add_errors(&mut self, plugin_type: SoftwareType, modules: Vec<SoftwareModuleItem>) {
        self.status = SoftwareOperationStatus::Failed;

        self.failures.push(SoftwareRequestResponseSoftwareList {
            plugin_type,
            modules,
        })
    }

    pub fn modules(&self) -> Vec<SoftwareModule> {
        let mut modules = vec![];

        if let Some(list) = &self.current_software_list {
            for module_per_plugin in list.iter() {
                let module_type = &module_per_plugin.plugin_type;
                for module in module_per_plugin.modules.iter() {
                    modules.push(SoftwareModule {
                        module_type: Some(module_type.clone()),
                        name: module.name.clone(),
                        version: module.version.clone(),
                        url: module.url.clone(),
                        file_path: None,
                    });
                }
            }
        }

        modules
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

impl From<SoftwareError> for Option<SoftwareModuleItem> {
    fn from(error: SoftwareError) -> Self {
        match error {
            SoftwareError::Install { module, reason } => Some(SoftwareModuleItem {
                name: module.name,
                version: module.version,
                url: module.url,
                action: Some(SoftwareModuleAction::Install),
                reason: Some(reason),
            }),
            SoftwareError::Remove { module, reason } => Some(SoftwareModuleItem {
                name: module.name,
                version: module.version,
                url: module.url,
                action: Some(SoftwareModuleAction::Remove),
                reason: Some(reason),
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_software_request_list() {
        let request = SoftwareListRequest {
            id: "1234".to_string(),
        };
        let expected_json = r#"{"id":"1234"}"#;

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
            modules: vec![debian_module1, debian_module2],
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
            modules: vec![docker_module1],
        };

        let request = SoftwareUpdateRequest {
            id: "1234".to_string(),
            update_list: vec![debian_list, docker_list],
        };

        let expected_json = r#"{"id":"1234","updateList":[{"type":"debian","modules":[{"name":"debian1","version":"0.0.1","action":"install"},{"name":"debian2","version":"0.0.2","action":"install"}]},{"type":"docker","modules":[{"name":"docker1","version":"0.0.1","url":"test.com","action":"remove"}]}]}"#;

        let actual_json = request.to_json().expect("Fail to serialize the request");
        assert_eq!(actual_json, expected_json);

        let parsed_request =
            SoftwareUpdateRequest::from_json(&actual_json).expect("Fail to parse the json request");
        assert_eq!(parsed_request, request);
    }

    #[test]
    fn serde_software_list_empty_successful() {
        let request = SoftwareRequestResponse {
            id: "1234".to_string(),
            status: SoftwareOperationStatus::Successful,
            reason: None,
            current_software_list: Some(vec![]),
            failures: vec![],
        };

        let expected_json = r#"{"id":"1234","status":"successful","currentSoftwareList":[]}"#;

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
            modules: vec![module1],
        };

        let request = SoftwareRequestResponse {
            id: "1234".to_string(),
            status: SoftwareOperationStatus::Successful,
            reason: None,
            current_software_list: Some(vec![docker_module1]),
            failures: vec![],
        };

        let expected_json = r#"{"id":"1234","status":"successful","currentSoftwareList":[{"type":"debian","modules":[{"name":"debian1","version":"0.0.1"}]}]}"#;

        let actual_json = request.to_json().expect("Fail to serialize the request");
        assert_eq!(actual_json, expected_json);

        let parsed_request = SoftwareRequestResponse::from_json(&actual_json)
            .expect("Fail to parse the json request");
        assert_eq!(parsed_request, request);
    }
}
