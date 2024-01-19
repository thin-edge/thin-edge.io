use crate::smartrest::error::SmartRestDeserializerError;
use csv::ReaderBuilder;
use serde::de::Error;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use std::fmt::Display;
use std::fmt::Formatter;
use time::format_description;
use time::OffsetDateTime;

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

pub fn to_datetime<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
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

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SmartRestLogRequest {
    pub message_id: String,
    pub device: String,
    pub log_type: String,
    #[serde(deserialize_with = "to_datetime")]
    pub date_from: OffsetDateTime,
    #[serde(deserialize_with = "to_datetime")]
    pub date_to: OffsetDateTime,
    pub search_text: Option<String>,
    pub lines: usize,
}

impl SmartRestRequestGeneric for SmartRestLogRequest {}

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
    use test_case::test_case;

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
    fn deserialize_incorrect_smartrest_message_id() {
        let smartrest = String::from("516,external_id");
        assert!(SmartRestConfigUploadRequest::from_smartrest(&smartrest).is_err());
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
