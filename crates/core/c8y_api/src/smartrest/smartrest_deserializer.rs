use crate::smartrest::error::SmartRestDeserializerError;
use csv::ReaderBuilder;
use download::DownloadInfo;
use serde::de::Error;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt::Display;
use std::fmt::Formatter;
use tedge_api::SoftwareModule;
use tedge_api::SoftwareModuleUpdate;
use tedge_api::SoftwareUpdateRequest;
use time::format_description;
use time::OffsetDateTime;

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
            param => Err(SmartRestDeserializerError::InvalidParameter {
                parameter: param.into(),
                operation: "c8y_SoftwareUpdate".into(),
                hint: "It must be install or delete.".into(),
            }),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SmartRestUpdateSoftware {
    pub message_id: String,
    pub external_id: String,
    pub update_list: Vec<SmartRestUpdateSoftwareModule>,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SmartRestUpdateSoftwareModule {
    pub software: String,
    pub version: Option<String>,
    pub url: Option<String>,
    pub action: String,
}

impl Default for SmartRestUpdateSoftware {
    fn default() -> Self {
        Self {
            message_id: "528".into(),
            external_id: "".into(),
            update_list: vec![],
        }
    }
}

impl SmartRestUpdateSoftware {
    pub fn from_smartrest(&self, smartrest: &str) -> Result<Self, SmartRestDeserializerError> {
        let mut message_id = smartrest.to_string();
        message_id.truncate(3);

        let mut rdr = ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(smartrest.as_bytes());
        let mut record: Self = Self::default();

        for result in rdr.deserialize() {
            record = result?;
        }

        Ok(record)
    }

    pub fn to_thin_edge_json(&self) -> Result<SoftwareUpdateRequest, SmartRestDeserializerError> {
        let request = self.map_to_software_update_request(SoftwareUpdateRequest::default())?;
        Ok(request)
    }

    pub fn modules(&self) -> &Vec<SmartRestUpdateSoftwareModule> {
        &self.update_list
    }

    fn map_to_software_update_request(
        &self,
        mut request: SoftwareUpdateRequest,
    ) -> Result<SoftwareUpdateRequest, SmartRestDeserializerError> {
        for module in self.modules() {
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

fn fix_timezone_offset(time: &str) -> String {
    let str_size = time.len();
    let split = time.split(['+', '-']).last();
    match split {
        Some(value) if !value.contains(':') => {
            time[0..str_size - 2].to_string() + ":" + &time[str_size - 2..str_size]
        }
        _ => time.to_string(),
    }
}

fn to_datetime<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    // NOTE `OffsetDateTime` is used here because c8y uses for log requests a date time string which is not compliant with rfc3339
    // c8y result:
    // 2021-10-23T19:03:26+0100
    // rfc3339 expected:
    // 2021-10-23T19:03:26+01:00
    // so we add a ':'
    let mut date_string: String = Deserialize::deserialize(deserializer)?;

    if date_string.contains('T') {
        let mut split = date_string.split('T');
        let mut date_part = split.next().unwrap().to_string(); // safe.

        let maybe_time_part = split.next();

        if let Some(time_part) = maybe_time_part {
            let time_part = fix_timezone_offset(time_part);
            date_part.push('T');
            date_part.push_str(&time_part);
        }
        date_string = date_part;
    }

    match OffsetDateTime::parse(&date_string, &format_description::well_known::Rfc3339) {
        Ok(result) => Ok(result),
        Err(e) => Err(D::Error::custom(&format!("Error: {}", e))),
    }
}

pub trait SmartRestRequestGeneric {
    fn from_smartrest(smartrest: &str) -> Result<Self, SmartRestDeserializerError>
    where
        Self: Sized,
        for<'de> Self: serde::Deserialize<'de>,
    {
        let mut rdr = ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(smartrest.as_bytes());

        rdr.deserialize()
            .next()
            .ok_or(SmartRestDeserializerError::EmptyRequest)?
            .map_err(SmartRestDeserializerError::from)
    }
}

pub enum SmartRestVariant {
    SmartRestLogRequest,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SmartRestLogRequest {
    pub message_id: String,
    pub device: String,
    pub log_type: String,
    #[serde(deserialize_with = "to_datetime")]
    pub date_from: OffsetDateTime,
    #[serde(deserialize_with = "to_datetime")]
    pub date_to: OffsetDateTime,
    pub needle: Option<String>,
    pub lines: usize,
}

impl SmartRestRequestGeneric for SmartRestLogRequest {}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SmartRestRestartRequest {
    pub message_id: String,
    pub device: String,
}

impl SmartRestRequestGeneric for SmartRestRestartRequest {}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SmartRestConfigUploadRequest {
    pub message_id: String,
    pub device: String,
    pub config_type: String,
}

impl SmartRestRequestGeneric for SmartRestConfigUploadRequest {}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
pub struct SmartRestConfigDownloadRequest {
    pub message_id: String,
    pub device: String,
    pub url: String,
    pub config_type: String,
}

impl SmartRestRequestGeneric for SmartRestConfigDownloadRequest {}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
pub struct SmartRestFirmwareRequest {
    pub message_id: String,
    pub device: String,
    pub name: String,
    pub version: String,
    pub url: String,
}

impl Display for SmartRestFirmwareRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "device={}, name={}, version={}, url={}",
            self.device, self.name, self.version, self.url
        )
    }
}

impl SmartRestRequestGeneric for SmartRestFirmwareRequest {}

type JwtToken = String;

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct SmartRestJwtResponse {
    id: u16,
    token: JwtToken,
}

impl Default for SmartRestJwtResponse {
    fn default() -> Self {
        Self {
            id: 71,
            token: "".into(),
        }
    }
}

impl SmartRestJwtResponse {
    pub fn try_new(to_parse: &str) -> Result<Self, SmartRestDeserializerError> {
        let mut csv = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(to_parse.as_bytes());

        let mut jwt = Self::default();
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

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct AvailableChildDevices {
    pub message_id: String,
    #[serde(default)]
    pub devices: std::collections::HashSet<String>,
}

impl SmartRestRequestGeneric for AvailableChildDevices {}

impl Default for AvailableChildDevices {
    fn default() -> Self {
        Self {
            message_id: "106".into(),
            devices: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_json_diff::*;
    use serde_json::json;
    use tedge_api::*;
    use test_case::test_case;

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
        let jwt = SmartRestJwtResponse::default();

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
        let update_software = SmartRestUpdateSoftware::default()
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
        assert!(SmartRestUpdateSoftware::default()
            .from_smartrest(&smartrest)
            .is_err());
    }

    #[test]
    fn deserialize_incorrect_smartrest_action() {
        let smartrest =
            String::from("528,external_id,software1,version1,url1,action,software2,,,remove");
        assert!(SmartRestUpdateSoftware::default()
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
        expected_thin_edge_json.add_update(SoftwareModuleUpdate::install(SoftwareModule {
            module_type: Some("debian".to_string()),
            name: "software1".to_string(),
            version: Some("version1".to_string()),
            url: Some("url1".into()),
            file_path: None,
        }));
        expected_thin_edge_json.add_update(SoftwareModuleUpdate::remove(SoftwareModule {
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
        let update_software = SmartRestUpdateSoftware::default();
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
        let update_software = SmartRestUpdateSoftware::default();
        let update_software = update_software.from_smartrest(&smartrest).unwrap();

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

        assert_eq!(update_software.modules(), &expected_vec);
    }

    #[test_case("2021-09-21T11:40:27+0200", "2021-09-22T11:40:27+0200"; "c8y expected")]
    #[test_case("2021-09-21T11:40:27+02:00", "2021-09-22T11:40:27+02:00"; "with colon both")]
    #[test_case("2021-09-21T11:40:27+02:00", "2021-09-22T11:40:27+0200"; "with colon date from")]
    #[test_case("2021-09-21T11:40:27+0200", "2021-09-22T11:40:27+02:00"; "with colon date to")]
    #[test_case("2021-09-21T11:40:27-0000", "2021-09-22T11:40:27-02:00"; "with negative timezone offset")]
    #[test_case("2021-09-21T11:40:27Z", "2021-09-22T11:40:00Z"; "utc timezone")]
    fn deserialize_smartrest_log_file_request_operation(date_from: &str, date_to: &str) {
        let smartrest = String::from(&format!(
            "522,DeviceSerial,syslog,{},{},ERROR,1000",
            date_from, date_to
        ));
        let log = SmartRestLogRequest::from_smartrest(&smartrest);
        assert!(log.is_ok());
    }

    #[test]
    fn deserialize_smartrest_restart_request_operation() {
        let smartrest = "510,user".to_string();
        let log = SmartRestRestartRequest::from_smartrest(&smartrest);
        assert!(log.is_ok());
    }

    #[test]
    fn deserialize_smartrest_config_upload_request() {
        let message_id = "526".to_string();
        let device = "deviceId".to_string();
        let config_type = "/test/config/path".to_string();

        let smartrest_message = format!("{message_id},{device},{config_type}");
        let expected = SmartRestConfigUploadRequest {
            message_id,
            device,
            config_type,
        };
        assert_eq!(
            SmartRestConfigUploadRequest::from_smartrest(smartrest_message.as_str()).unwrap(),
            expected
        );
    }

    #[test]
    fn deserialize_smartrest_config_download_request_operation() {
        let smartrest = "524,deviceId,https://test.cumulocity.com/inventory/binaries/70208,/etc/tedge/tedge.toml".to_string();
        let request = SmartRestConfigDownloadRequest::from_smartrest(&smartrest).unwrap();
        let expected_output = SmartRestConfigDownloadRequest {
            message_id: "524".to_string(),
            device: "deviceId".to_string(),
            url: "https://test.cumulocity.com/inventory/binaries/70208".to_string(),
            config_type: "/etc/tedge/tedge.toml".to_string(),
        };
        assert_eq!(request, expected_output);
    }

    #[test]
    fn deserialize_smartrest_firmware_request_operation() {
        let smartrest = "515,DeviceSerial,myFirmware,1.0,http://www.my.url".to_string();
        let request = SmartRestFirmwareRequest::from_smartrest(&smartrest).unwrap();
        let expected_output = SmartRestFirmwareRequest {
            message_id: "515".to_string(),
            device: "DeviceSerial".to_string(),
            name: "myFirmware".to_string(),
            version: "1.0".to_string(),
            url: "http://www.my.url".to_string(),
        };
        assert_eq!(request, expected_output);
    }
}
