pub mod apt_config;
pub mod auto;
pub mod c8y_software_management;
pub mod connect_url;
pub mod flag;
pub mod host_port;
pub mod ipaddress;
pub mod port;
pub mod seconds;
pub mod templates_set;

use std::str::FromStr;
use strum::Display;

pub const HTTPS_PORT: u16 = 443;
pub const MQTT_TLS_PORT: u16 = 8883;

pub use self::apt_config::*;
pub use self::auto::*;
pub use self::c8y_software_management::*;
pub use self::connect_url::*;
pub use self::flag::*;
#[doc(inline)]
pub use self::host_port::HostPort;
pub use self::ipaddress::*;
pub use self::port::*;
pub use self::seconds::*;
pub use self::templates_set::*;
pub use tedge_utils::timestamp;

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
