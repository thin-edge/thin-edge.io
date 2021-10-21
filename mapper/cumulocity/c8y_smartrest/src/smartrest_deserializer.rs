use crate::error::SmartRestDeserializerError;
use csv::ReaderBuilder;
use json_sm::{DownloadInfo, SoftwareModule, SoftwareModuleUpdate, SoftwareUpdateRequest};
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};

#[derive(Debug)]
enum CumulocitySoftwareUpdateActions {
    Install,
    Delete,
}

impl TryFrom<String> for CumulocitySoftwareUpdateActions {
    type Error = SmartRestDeserializerError;

    fn try_from(action: String) -> Result<Self, Self::Error> {
        match action.as_str() {
            "install" => Ok(Self::Install),
            "delete" => Ok(Self::Delete),
            _ => Err(SmartRestDeserializerError::ActionNotFound { action }),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SmartRestUpdateSoftware {
    pub message_id: String,
    pub external_id: String,
    pub update_list: Vec<SmartRestUpdateSoftwareModule>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SmartRestUpdateSoftwareModule {
    pub software: String,
    pub version: Option<String>,
    pub url: Option<String>,
    pub action: String,
}

impl SmartRestUpdateSoftware {
    pub fn new() -> Self {
        Self {
            message_id: "528".into(),
            external_id: "".into(),
            update_list: vec![],
        }
    }

    pub fn from_smartrest(&self, smartrest: &str) -> Result<Self, SmartRestDeserializerError> {
        let mut message_id = smartrest.to_string();
        let () = message_id.truncate(3);
        //if message_id != self.message_id {
        //    return Err(SmartRestDeserializerError::UnsupportedOperation { id: message_id });
        //}

        let mut rdr = ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(smartrest.as_bytes());
        let mut record: Self = Self::new();
        for result in rdr.deserialize() {
            record = result?;
        }
        Ok(record)
    }

    pub fn to_thin_edge_json(&self) -> Result<SoftwareUpdateRequest, SmartRestDeserializerError> {
        let request = self.map_to_software_update_request(SoftwareUpdateRequest::new())?;
        Ok(request)
    }

    pub fn modules(&self) -> Vec<SmartRestUpdateSoftwareModule> {
        let mut modules = vec![];
        for module in &self.update_list {
            modules.push(SmartRestUpdateSoftwareModule {
                software: module.software.clone(),
                version: module.version.clone(),
                url: module.url.clone(),
                action: module.action.clone(),
            });
        }
        modules
    }

    fn map_to_software_update_request(
        &self,
        mut request: SoftwareUpdateRequest,
    ) -> Result<SoftwareUpdateRequest, SmartRestDeserializerError> {
        for module in &self.modules() {
            match module.action.clone().try_into()? {
                CumulocitySoftwareUpdateActions::Install => {
                    request.add_update(SoftwareModuleUpdate::Install {
                        module: SoftwareModule {
                            module_type: module.get_module_version_and_type().1,
                            name: module.software.clone(),
                            version: module.get_module_version_and_type().0,
                            url: module.get_url(),
                            file_path: None,
                        },
                    });
                }
                CumulocitySoftwareUpdateActions::Delete => {
                    request.add_update(SoftwareModuleUpdate::Remove {
                        module: SoftwareModule {
                            module_type: module.get_module_version_and_type().1,
                            name: module.software.clone(),
                            version: module.get_module_version_and_type().0,
                            url: None,
                            file_path: None,
                        },
                    });
                }
            }
        }
        Ok(request)
    }
}

impl SmartRestUpdateSoftwareModule {
    fn get_module_version_and_type(&self) -> (Option<String>, Option<String>) {
        let split;
        match &self.version {
            Some(version) => {
                if version.matches("::").count() > 1 {
                    split = version.rsplit_once("::");
                } else {
                    split = version.split_once("::");
                }

                match split {
                    Some((v, t)) => {
                        if v.is_empty() {
                            (None, Some(t.into())) // ::debian
                        } else if !t.is_empty() {
                            (Some(v.into()), Some(t.into())) // 1.0::debian
                        } else {
                            (Some(v.into()), None)
                        }
                    }
                    None => {
                        if version == " " {
                            (None, None) // as long as c8y UI forces version input
                        } else {
                            (Some(version.into()), None) // 1.0
                        }
                    }
                }
            }

            None => (None, None), // (empty)
        }
    }

    fn get_url(&self) -> Option<DownloadInfo> {
        match &self.url {
            Some(url) if url.trim().is_empty() => None,
            Some(url) => Some(DownloadInfo::new(url.as_str())),
            None => None,
        }
    }
}

type JwtToken = String;

#[derive(Debug, Deserialize, PartialEq)]
pub struct SmartRestJwtResponse {
    id: u16,
    token: JwtToken,
}

impl SmartRestJwtResponse {
    pub fn new() -> Self {
        Self {
            id: 71,
            token: "".into(),
        }
    }

    pub fn try_new(to_parse: &str) -> Result<Self, SmartRestDeserializerError> {
        let mut csv = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(to_parse.as_bytes());

        let mut jwt = Self::new();
        for result in csv.deserialize() {
            jwt = result.unwrap();
        }

        if jwt.id != 71 {
            return Err(SmartRestDeserializerError::InvalidMessageId(jwt.id));
        }

        Ok(jwt)
    }

    pub fn token(&self) -> JwtToken {
        self.token.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_json_diff::*;
    use json_sm::*;
    use serde_json::json;

    // To avoid using an ID randomly generated, which is not convenient for testing.
    impl SmartRestUpdateSoftware {
        fn to_thin_edge_json_with_id(
            &self,
            id: &str,
        ) -> Result<SoftwareUpdateRequest, SmartRestDeserializerError> {
            let request = SoftwareUpdateRequest::new_with_id(id);
            self.map_to_software_update_request(request)
        }
    }

    #[test]
    fn jwt_token_create_new() {
        let jwt = SmartRestJwtResponse::new();

        assert!(jwt.token.is_empty());
    }

    #[test]
    fn jwt_token_deserialize_correct_id_returns_token() {
        let test_response = "71,123456";
        let jwt = SmartRestJwtResponse::try_new(test_response).unwrap();

        assert_eq!(jwt.token(), "123456");
    }

    #[test]
    fn jwt_token_deserialize_incorrect_id_returns_error() {
        let test_response = "42,123456";

        let jwt = SmartRestJwtResponse::try_new(test_response);

        assert!(jwt.is_err());
        assert_matches::assert_matches!(jwt, Err(SmartRestDeserializerError::InvalidMessageId(42)));
    }

    #[test]
    fn verify_get_module_version_and_type() {
        let mut module = SmartRestUpdateSoftwareModule {
            software: "software1".into(),
            version: None,
            url: None,
            action: "install".into(),
        }; // ""
        assert_eq!(module.get_module_version_and_type(), (None, None));

        module.version = Some(" ".into()); // " " (space)
        assert_eq!(module.get_module_version_and_type(), (None, None));

        module.version = Some("::debian".into());
        assert_eq!(
            module.get_module_version_and_type(),
            (None, Some("debian".to_string()))
        );

        module.version = Some("1.0.0::debian".into());
        assert_eq!(
            module.get_module_version_and_type(),
            (Some("1.0.0".to_string()), Some("debian".to_string()))
        );

        module.version = Some("1.0.0::1::debian".into());
        assert_eq!(
            module.get_module_version_and_type(),
            (Some("1.0.0::1".to_string()), Some("debian".to_string()))
        );

        module.version = Some("1.0.0::1::".into());
        assert_eq!(
            module.get_module_version_and_type(),
            (Some("1.0.0::1".to_string()), None)
        );

        module.version = Some("1.0.0".into());
        assert_eq!(
            module.get_module_version_and_type(),
            (Some("1.0.0".to_string()), None)
        );
    }

    #[test]
    fn deserialize_smartrest_update_software() {
        let smartrest =
            String::from("528,external_id,software1,version1,url1,install,software2,,,delete");
        let update_software = SmartRestUpdateSoftware::new()
            .from_smartrest(&smartrest)
            .unwrap();

        let expected_update_software = SmartRestUpdateSoftware {
            message_id: "528".into(),
            external_id: "external_id".into(),
            update_list: vec![
                SmartRestUpdateSoftwareModule {
                    software: "software1".into(),
                    version: Some("version1".into()),
                    url: Some("url1".into()),
                    action: "install".into(),
                },
                SmartRestUpdateSoftwareModule {
                    software: "software2".into(),
                    version: None,
                    url: None,
                    action: "delete".into(),
                },
            ],
        };

        assert_eq!(update_software, expected_update_software);
    }

    #[test]
    fn deserialize_incorrect_smartrest_message_id() {
        let smartrest = String::from("516,external_id");
        assert!(SmartRestUpdateSoftware::new()
            .from_smartrest(&smartrest)
            .is_err());
    }

    #[test]
    fn deserialize_incorrect_smartrest_action() {
        let smartrest =
            String::from("528,external_id,software1,version1,url1,action,software2,,,remove");
        assert!(SmartRestUpdateSoftware::new()
            .from_smartrest(&smartrest)
            .unwrap()
            .to_thin_edge_json()
            .is_err());
    }

    #[test]
    fn from_smartrest_update_software_to_software_update_request() {
        let smartrest_obj = SmartRestUpdateSoftware {
            message_id: "528".into(),
            external_id: "external_id".into(),
            update_list: vec![
                SmartRestUpdateSoftwareModule {
                    software: "software1".into(),
                    version: Some("version1::debian".into()),
                    url: Some("url1".into()),
                    action: "install".into(),
                },
                SmartRestUpdateSoftwareModule {
                    software: "software2".into(),
                    version: None,
                    url: None,
                    action: "delete".into(),
                },
            ],
        };
        let thin_edge_json = smartrest_obj.to_thin_edge_json_with_id("123").unwrap();

        let mut expected_thin_edge_json = SoftwareUpdateRequest::new_with_id("123");
        let () =
            expected_thin_edge_json.add_update(SoftwareModuleUpdate::install(SoftwareModule {
                module_type: Some("debian".to_string()),
                name: "software1".to_string(),
                version: Some("version1".to_string()),
                url: Some("url1".into()),
                file_path: None,
            }));
        let () = expected_thin_edge_json.add_update(SoftwareModuleUpdate::remove(SoftwareModule {
            module_type: Some("".to_string()),
            name: "software2".to_string(),
            version: None,
            url: None,
            file_path: None,
        }));

        assert_eq!(thin_edge_json, expected_thin_edge_json);
    }

    #[test]
    fn from_smartrest_update_software_to_json() {
        let smartrest =
            String::from("528,external_id,nodered,1.0.0::debian,,install,\
            collectd,5.7::debian,https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2,install,\
            nginx,1.21.0::docker,,install,mongodb,4.4.6::docker,,delete");
        let update_software = SmartRestUpdateSoftware::new();
        let software_update_request = update_software
            .from_smartrest(&smartrest)
            .unwrap()
            .to_thin_edge_json_with_id("123");
        let output_json = software_update_request.unwrap().to_json().unwrap();

        let expected_json = json!({
            "id": "123",
            "updateList": [
                {
                    "type": "debian",
                    "modules": [
                        {
                            "name": "nodered",
                            "version": "1.0.0",
                            "action": "install"
                        },
                        {
                            "name": "collectd",
                            "version": "5.7",
                            "url": "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2",
                            "action": "install"
                        }
                    ]
                },
                {
                    "type": "docker",
                    "modules": [
                        {
                            "name": "nginx",
                            "version": "1.21.0",
                            "action": "install"
                        },
                        {
                            "name": "mongodb",
                            "version": "4.4.6",
                            "action": "remove"
                        }
                    ]
                }
            ]
        });
        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(output_json.as_str()).unwrap(),
            expected_json
        );
    }

    #[test]
    fn access_smartrest_update_modules() {
        let smartrest =
            String::from("528,external_id,software1,version1,url1,install,software2,,,delete");
        let update_software = SmartRestUpdateSoftware::new();
        let vec = update_software
            .from_smartrest(&smartrest)
            .unwrap()
            .modules();

        let expected_vec = vec![
            SmartRestUpdateSoftwareModule {
                software: "software1".into(),
                version: Some("version1".into()),
                url: Some("url1".into()),
                action: "install".into(),
            },
            SmartRestUpdateSoftwareModule {
                software: "software2".into(),
                version: None,
                url: None,
                action: "delete".into(),
            },
        ];

        assert_eq!(vec, expected_vec);
    }
}
