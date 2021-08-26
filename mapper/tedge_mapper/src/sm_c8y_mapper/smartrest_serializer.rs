use crate::sm_c8y_mapper::error::SmartRestSerializerError;
use csv::{QuoteStyle, WriterBuilder};
use json_sm::{
    SoftwareListResponse, SoftwareOperationStatus, SoftwareType, SoftwareUpdateResponse,
    SoftwareVersion,
};
use serde::{Deserialize, Serialize, Serializer};

type SmartRest = String;

#[derive(Debug)]
pub(crate) enum CumulocitySupportedOperations {
    C8ySoftwareUpdate,
}

impl From<CumulocitySupportedOperations> for &'static str {
    fn from(op: CumulocitySupportedOperations) -> Self {
        match op {
            CumulocitySupportedOperations::C8ySoftwareUpdate => "c8y_SoftwareUpdate",
        }
    }
}

pub(crate) trait SmartRestSerializer<'a>
where
    Self: Serialize,
{
    fn to_smartrest(&self) -> Result<SmartRest, SmartRestSerializerError> {
        serialize_smartrest(self)
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct SmartRestSetSupportedOperations {
    pub message_id: &'static str,
    pub supported_operations: Vec<&'static str>,
}

impl Default for SmartRestSetSupportedOperations {
    fn default() -> Self {
        Self {
            message_id: "114",
            supported_operations: vec![CumulocitySupportedOperations::C8ySoftwareUpdate.into()],
        }
    }
}

impl<'a> SmartRestSerializer<'a> for SmartRestSetSupportedOperations {}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct SmartRestSetSoftwareList {
    pub message_id: &'static str,
    pub software_list: Vec<SmartRestSoftwareModuleItem>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct SmartRestSoftwareModuleItem {
    pub software: String,
    pub version: Option<String>,
    pub url: Option<String>,
}

impl SmartRestSetSoftwareList {
    pub(crate) fn new(list: Vec<SmartRestSoftwareModuleItem>) -> Self {
        Self {
            message_id: "116",
            software_list: list,
        }
    }

    pub(crate) fn from_thin_edge_json(response: SoftwareListResponse) -> Self {
        let modules = response.modules();
        let mut list: Vec<SmartRestSoftwareModuleItem> = Vec::new();
        for module in modules {
            let item = SmartRestSoftwareModuleItem {
                software: module.name,
                version: Option::from(combine_version_and_type(
                    &module.version,
                    &module.module_type,
                )),
                url: module.url,
            };
            list.push(item);
        }
        Self::new(list)
    }
}

impl<'a> SmartRestSerializer<'a> for SmartRestSetSoftwareList {}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct SmartRestGetPendingOperations {
    pub id: &'static str,
}

impl Default for SmartRestGetPendingOperations {
    fn default() -> Self {
        Self { id: "500" }
    }
}

impl<'a> SmartRestSerializer<'a> for SmartRestGetPendingOperations {}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct SmartRestSetOperationToExecuting {
    pub message_id: &'static str,
    pub operation: &'static str,
}

impl SmartRestSetOperationToExecuting {
    pub(crate) fn new(operation: CumulocitySupportedOperations) -> Self {
        Self {
            message_id: "501",
            operation: operation.into(),
        }
    }

    pub(crate) fn from_thin_edge_json(
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
pub(crate) struct SmartRestSetOperationToSuccessful {
    pub message_id: &'static str,
    pub operation: &'static str,
}

impl SmartRestSetOperationToSuccessful {
    fn new(operation: CumulocitySupportedOperations) -> Self {
        Self {
            message_id: "503",
            operation: operation.into(),
        }
    }

    pub(crate) fn from_thin_edge_json(
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
pub(crate) struct SmartRestSetOperationToFailed {
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

    pub(crate) fn from_thin_edge_json(
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

fn combine_version_and_type(
    version: &Option<SoftwareVersion>,
    module_type: &Option<SoftwareType>,
) -> String {
    match module_type {
        Some(m) => {
            if m.is_empty() {
                match version {
                    Some(v) => v.to_string(),
                    None => "".to_string(),
                }
            } else {
                match version {
                    Some(v) => format!("{}::{}", v, m),
                    None => format!("::{}", m),
                }
            }
        }
        None => match version {
            Some(v) => {
                if v.contains("::") {
                    format!("{}::", v)
                } else {
                    v.to_string()
                }
            }
            None => "".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use json_sm::*;

    #[test]
    fn verify_combine_version_and_type() {
        let some_version: Option<SoftwareVersion> = Some("1.0".to_string());
        let some_version_with_colon: Option<SoftwareVersion> = Some("1.0.0::1".to_string());
        let none_version: Option<SoftwareVersion> = None;
        let some_module_type: Option<SoftwareType> = Some("debian".to_string());
        let none_module_type: Option<SoftwareType> = None;

        assert_eq!(
            combine_version_and_type(&some_version, &some_module_type),
            "1.0::debian"
        );
        assert_eq!(
            combine_version_and_type(&some_version, &none_module_type),
            "1.0"
        );
        assert_eq!(
            combine_version_and_type(&some_version_with_colon, &some_module_type),
            "1.0.0::1::debian"
        );
        assert_eq!(
            combine_version_and_type(&some_version_with_colon, &none_module_type),
            "1.0.0::1::"
        );
        assert_eq!(
            combine_version_and_type(&none_version, &some_module_type),
            "::debian"
        );
        assert_eq!(
            combine_version_and_type(&none_version, &none_module_type),
            ""
        );
    }

    #[test]
    fn serialize_smartrest_supported_operations() {
        let smartrest = SmartRestSetSupportedOperations::default()
            .to_smartrest()
            .unwrap();
        assert_eq!(smartrest, "114,c8y_SoftwareUpdate\n");
    }

    #[test]
    fn serialize_smartrest_set_software_list() {
        let smartrest = SmartRestSetSoftwareList::new(vec![
            SmartRestSoftwareModuleItem {
                software: "software1".into(),
                version: Some("0.1.0".into()),
                url: Some("https://test.com".into()),
            },
            SmartRestSoftwareModuleItem {
                software: "software2".into(),
                version: None,
                url: None,
            },
        ])
        .to_smartrest()
        .unwrap();
        assert_eq!(
            smartrest,
            "116,software1,0.1.0,https://test.com,software2,,\n"
        );
    }

    #[test]
    fn from_thin_edge_json_to_smartrest_object_set_software_list() {
        let input_json = r#"{
            "id":"123",
            "status":"successful",
            "currentSoftwareList":[
                {"type":"debian", "modules":[
                    {"name":"a"},
                    {"name":"b","version":"1.0"},
                    {"name":"c","url":"https://foobar.io/c.deb"},
                    {"name":"d","version":"beta","url":"https://foobar.io/d.deb"}
                ]},
                {"type":"","modules":[
                    {"name":"m","url":"https://foobar.io/m.epl"}
                ]}
            ]}"#;

        let json_obj = SoftwareListResponse::from_json(input_json).unwrap();
        let smartrest_obj = SmartRestSetSoftwareList::from_thin_edge_json(json_obj);

        let expected_smartrest_obj = SmartRestSetSoftwareList {
            message_id: "116",
            software_list: vec![
                SmartRestSoftwareModuleItem {
                    software: "a".to_string(),
                    version: Some("::debian".to_string()),
                    url: None,
                },
                SmartRestSoftwareModuleItem {
                    software: "b".to_string(),
                    version: Some("1.0::debian".to_string()),
                    url: None,
                },
                SmartRestSoftwareModuleItem {
                    software: "c".to_string(),
                    version: Some("::debian".to_string()),
                    url: Some("https://foobar.io/c.deb".to_string()),
                },
                SmartRestSoftwareModuleItem {
                    software: "d".to_string(),
                    version: Some("beta::debian".to_string()),
                    url: Some("https://foobar.io/d.deb".to_string()),
                },
                SmartRestSoftwareModuleItem {
                    software: "m".to_string(),
                    version: Some("".to_string()),
                    url: Some("https://foobar.io/m.epl".to_string()),
                },
            ],
        };
        assert_eq!(smartrest_obj, expected_smartrest_obj);
    }

    #[test]
    fn from_thin_edge_json_to_smartrest_set_software_list() {
        let input_json = r#"{
            "id":"1",
            "status":"successful",
            "currentSoftwareList":[
                {"type":"debian", "modules":[
                    {"name":"a"},
                    {"name":"b","version":"1.0"},
                    {"name":"c","url":"https://foobar.io/c.deb"},
                    {"name":"d","version":"beta","url":"https://foobar.io/d.deb"}
                ]},
                {"type":"apama","modules":[
                    {"name":"m","url":"https://foobar.io/m.epl"}
                ]}
            ]}"#;

        let json_obj = SoftwareListResponse::from_json(input_json).unwrap();
        let smartrest = SmartRestSetSoftwareList::from_thin_edge_json(json_obj)
            .to_smartrest()
            .unwrap();

        let expected_smartrest= "116,a,::debian,,b,1.0::debian,,c,::debian,https://foobar.io/c.deb,d,beta::debian,https://foobar.io/d.deb,m,::apama,https://foobar.io/m.epl\n";
        assert_eq!(smartrest, expected_smartrest.to_string());
    }

    #[test]
    fn empty_to_smartrest_set_software_list() {
        let input_json = r#"{
            "id":"1",
            "status":"successful",
            "currentSoftwareList":[]
            }"#;

        let json_obj = SoftwareListResponse::from_json(input_json).unwrap();
        let smartrest = SmartRestSetSoftwareList::from_thin_edge_json(json_obj)
            .to_smartrest()
            .unwrap();

        let expected_smartrest = "116\n".to_string();
        assert_eq!(smartrest, expected_smartrest);
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
