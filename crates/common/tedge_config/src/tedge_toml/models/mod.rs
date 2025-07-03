pub mod apt_config;
pub mod auth_method;
pub mod auto;
pub mod c8y_software_management;
pub mod connect_url;
pub mod cryptoki;
pub mod flag;
pub mod host_port;
pub mod ipaddress;
pub mod path;
pub mod port;
pub mod proxy_scheme;
pub mod proxy_url;
pub mod seconds;
pub mod templates_set;
pub mod topic_prefix;

use doku::Document;
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Display;
use std::str::FromStr;
use strum::Display;

pub const HTTPS_PORT: u16 = 443;
pub const MQTT_TLS_PORT: u16 = 8883;
pub const MQTT_SVC_TLS_PORT: u16 = 9883;

pub use self::apt_config::*;
pub use self::auto::*;
pub use self::c8y_software_management::*;
pub use self::connect_url::*;
pub use self::cryptoki::Cryptoki;
pub use self::flag::*;
#[doc(inline)]
pub use self::host_port::HostPort;
pub use self::ipaddress::*;
pub use self::path::*;
pub use self::port::*;
pub use self::seconds::*;
pub use self::templates_set::*;
pub use tedge_utils::timestamp;
pub use tedge_utils::timestamp::TimeFormat;
pub use topic_prefix::TopicPrefix;

#[derive(
    Debug, Display, Clone, Copy, Eq, PartialEq, doku::Document, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum AutoLogUpload {
    Never,
    Always,
    OnFailure,
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to parse flag: {input}. Supported values are: 'never', 'always' or 'on-failure'")]
pub struct InvalidAutoLogUploadConfig {
    input: String,
}

impl FromStr for AutoLogUpload {
    type Err = InvalidAutoLogUploadConfig;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "never" => Ok(AutoLogUpload::Never),
            "always" => Ok(AutoLogUpload::Always),
            "on-failure" => Ok(AutoLogUpload::OnFailure),
            _ => Err(InvalidAutoLogUploadConfig {
                input: input.to_string(),
            }),
        }
    }
}

pub const MQTT_MAX_PAYLOAD_SIZE: u32 = 268435455;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Document)]
pub struct MqttPayloadLimit(pub u32);

#[derive(thiserror::Error, Debug)]
#[error("Invalid MQTT payload size limit: {0}. Provide a value between 0 and 268435455 bytes")]
pub struct InvalidMqttPayloadLimit(String);

impl FromStr for MqttPayloadLimit {
    type Err = InvalidMqttPayloadLimit;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let limit = value
            .parse::<u32>()
            .map_err(|_| InvalidMqttPayloadLimit(value.to_string()))?;
        limit.try_into()
    }
}

impl TryFrom<u32> for MqttPayloadLimit {
    type Error = InvalidMqttPayloadLimit;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value == 0 || value > MQTT_MAX_PAYLOAD_SIZE {
            return Err(InvalidMqttPayloadLimit(value.to_string()));
        }

        Ok(MqttPayloadLimit(value))
    }
}

impl Display for MqttPayloadLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::InvalidMqttPayloadLimit;
    use super::MqttPayloadLimit;
    use assert_matches::assert_matches;

    #[test]
    fn test_mqtt_payload_limit() {
        // Zero size is invalid
        let res = MqttPayloadLimit::try_from(0);
        assert_matches!(res, Err(InvalidMqttPayloadLimit(_)));

        // Values higher than 256 MB are also invalid
        let res = MqttPayloadLimit::try_from(268435455 + 1);
        assert_matches!(res, Err(InvalidMqttPayloadLimit(_)));

        // Max limit is valid
        let res = MqttPayloadLimit::try_from(268435455);
        assert_matches!(res.unwrap().0, 268435455);

        // Anything less than that is also valid
        let res = MqttPayloadLimit::try_from(268435455 - 1);
        assert_matches!(res.unwrap().0, 268435454);
    }
}
