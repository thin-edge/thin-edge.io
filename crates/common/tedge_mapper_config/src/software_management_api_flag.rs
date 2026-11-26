use std::fmt;
use std::str::FromStr;

/// A flag that switches legacy or advanced software management API
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[facet(proxy = String)]
#[repr(u8)]
pub enum SoftwareManagementApiFlag {
    Legacy,
    Advanced,
}

impl FromStr for SoftwareManagementApiFlag {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "legacy" => Ok(Self::Legacy),
            "advanced" => Ok(Self::Advanced),
            _ => Err(format!(
                "Failed to parse flag: {input}. Supported values are: legacy, advanced"
            )),
        }
    }
}

impl fmt::Display for SoftwareManagementApiFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Legacy => f.write_str("legacy"),
            Self::Advanced => f.write_str("advanced"),
        }
    }
}

/// Reads the flag from a file, where tedge_config also accepts it capitalised
impl TryFrom<String> for SoftwareManagementApiFlag {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "Legacy" => Ok(Self::Legacy),
            "Advanced" => Ok(Self::Advanced),
            other => other.parse(),
        }
    }
}

impl From<&SoftwareManagementApiFlag> for String {
    fn from(value: &SoftwareManagementApiFlag) -> Self {
        value.to_string()
    }
}

tedge_config_engine::impl_single_value_append_remove!(SoftwareManagementApiFlag);
