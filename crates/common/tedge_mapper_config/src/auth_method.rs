use std::fmt;
use std::str::FromStr;

/// Authentication method for the cloud broker connection.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
#[facet(rename_all = "lowercase")]
pub enum AuthMethod {
    #[default]
    Auto,
    Certificate,
    Password,
}

impl fmt::Display for AuthMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => f.write_str("auto"),
            Self::Certificate => f.write_str("certificate"),
            Self::Password => f.write_str("password"),
        }
    }
}

impl FromStr for AuthMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "certificate" => Ok(Self::Certificate),
            "password" | "basic" => Ok(Self::Password),
            _ => Err(format!(
                "unknown auth method '{s}', expected one of: auto, certificate, password"
            )),
        }
    }
}

tedge_config_engine::impl_single_value_append_remove!(AuthMethod);
