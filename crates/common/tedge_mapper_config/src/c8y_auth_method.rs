use std::fmt;
use std::str::FromStr;

/// How the Cumulocity mapper authenticates
///
/// Unlike [AuthMethod](crate::AuthMethod) of the generic schema, which is
/// written as `password`, Cumulocity's files and output call it `basic`
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
#[facet(rename_all = "lowercase")]
pub enum C8yAuthMethod {
    Certificate,
    Basic,
    Auto,
}

impl fmt::Display for C8yAuthMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Certificate => f.write_str("certificate"),
            Self::Basic => f.write_str("basic"),
            Self::Auto => f.write_str("auto"),
        }
    }
}

impl FromStr for C8yAuthMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "certificate" => Ok(Self::Certificate),
            "basic" | "password" => Ok(Self::Basic),
            "auto" => Ok(Self::Auto),
            _ => Err(format!(
                "Failed to parse flag: {s}. Supported values are: 'certificate', 'basic' or 'auto'"
            )),
        }
    }
}

tedge_config_engine::impl_single_value_append_remove!(C8yAuthMethod);
