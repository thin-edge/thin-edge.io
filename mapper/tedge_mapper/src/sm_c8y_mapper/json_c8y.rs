use json_sm::{Jsonify, SoftwareListResponse, SoftwareModule, SoftwareType, SoftwareVersion};
use serde::{Deserialize, Serialize};

const EMPTY_STRING: &str = "";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct C8yManagedObject {
    id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternalIdResponse {
    managed_object: C8yManagedObject,
    external_id: String,
}

impl InternalIdResponse {
    pub fn id(&self) -> String {
        self.managed_object.id.clone()
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct C8ySoftwareModuleItem {
    pub name: String,
    pub version: Option<String>,
    pub url: Option<String>,
}

impl<'a> Jsonify<'a> for C8ySoftwareModuleItem {}

impl From<SoftwareModule> for C8ySoftwareModuleItem {
    fn from(module: SoftwareModule) -> Self {
        let url = if module.url.is_none() {
            Some(EMPTY_STRING.into())
        } else {
            module.url
        };

        Self {
            name: module.name,
            version: Option::from(combine_version_and_type(
                &module.version,
                &module.module_type,
            )),
            url,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct C8yUpdateSoftwareListResponse {
    #[serde(rename = "c8y_SoftwareList")]
    c8y_software_list: Option<Vec<C8ySoftwareModuleItem>>,
}

impl<'a> Jsonify<'a> for C8yUpdateSoftwareListResponse {}

impl From<&SoftwareListResponse> for C8yUpdateSoftwareListResponse {
    fn from(list: &SoftwareListResponse) -> Self {
        let mut new_list: Vec<C8ySoftwareModuleItem> = Vec::new();
        list.modules().into_iter().for_each(|software_module| {
            let c8y_software_module: C8ySoftwareModuleItem = software_module.into();
            new_list.push(c8y_software_module);
        });

        Self {
            c8y_software_list: Some(new_list),
        }
    }
}

fn combine_version_and_type(
    version: &Option<SoftwareVersion>,
    module_type: &Option<SoftwareType>,
) -> String {
    match module_type {
        Some(m) => {
            if m.is_empty() {
                match version {
                    Some(v) => v.into(),
                    None => EMPTY_STRING.into(),
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
                    v.into()
                }
            }
            None => EMPTY_STRING.into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_software_module_to_c8y_software_module_item() {
        let software_module = SoftwareModule {
            module_type: Some("a".into()),
            name: "b".into(),
            version: Some("c".into()),
            url: Some("".into()),
        };

        let expected_c8y_item = C8ySoftwareModuleItem {
            name: "b".into(),
            version: Some("c::a".into()),
            url: Some("".into()),
        };

        let converted: C8ySoftwareModuleItem = software_module.into();

        assert_eq!(converted, expected_c8y_item);
    }

    #[test]
    fn get_id_from_c8y_response() {
        let managed_object = C8yManagedObject { id: "12345".into() };
        let response = InternalIdResponse {
            managed_object,
            external_id: "test".into(),
        };

        assert_eq!(response.id(), "12345".to_string());
    }

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
            EMPTY_STRING
        );
    }
}
