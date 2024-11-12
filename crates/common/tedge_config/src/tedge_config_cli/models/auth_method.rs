use camino::Utf8Path;
use std::str::FromStr;
use strum_macros::Display;

#[derive(
    Debug, Display, Clone, Copy, Eq, PartialEq, doku::Document, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum AuthMethod {
    Certificate,
    Basic,
    Auto,
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to parse flag: {input}. Supported values are: 'certificate', 'basic' or 'auto'")]
pub struct InvalidRegistrationMode {
    input: String,
}

impl FromStr for AuthMethod {
    type Err = InvalidRegistrationMode;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "certificate" => Ok(AuthMethod::Certificate),
            "basic" => Ok(AuthMethod::Basic),
            "auto" => Ok(AuthMethod::Auto),
            _ => Err(InvalidRegistrationMode {
                input: input.to_string(),
            }),
        }
    }
}

pub enum AuthType {
    Certificate,
    Basic,
}

impl AuthMethod {
    pub fn is_basic(self, credentials_path: &Utf8Path) -> bool {
        matches!(self.to_type(credentials_path), AuthType::Basic)
    }

    pub fn is_certificate(self, credentials_path: &Utf8Path) -> bool {
        matches!(self.to_type(credentials_path), AuthType::Certificate)
    }

    pub fn to_type(self, credentials_path: &Utf8Path) -> AuthType {
        match self {
            AuthMethod::Certificate => AuthType::Certificate,
            AuthMethod::Basic => AuthType::Basic,
            AuthMethod::Auto if credentials_path.exists() => AuthType::Basic,
            AuthMethod::Auto => AuthType::Certificate,
        }
    }
}

pub fn try_get_device_id_from_credentials_file(credentials_path: &Utf8Path) -> Option<String> {
    if let Ok(contents) = std::fs::read_to_string(credentials_path) {
        if let Ok(credentials) = toml::from_str::<CredentialsFile>(&contents) {
            return Some(credentials.c8y.device_id);
        }
    }
    None
}

#[derive(Debug, serde::Deserialize)]
struct CredentialsFile {
    c8y: C8y,
}

#[derive(Debug, serde::Deserialize)]
struct C8y {
    device_id: String,
}
