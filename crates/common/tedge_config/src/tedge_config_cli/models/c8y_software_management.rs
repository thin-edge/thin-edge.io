use std::str::FromStr;
use strum::Display;

/// A flag that switches legacy or advanced software management API.
/// Can be set to auto in the future, see #2778.
#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, Eq, PartialEq, doku::Document, Display,
)]
#[strum(serialize_all = "camelCase")]
pub enum SoftwareManagementApiFlag {
    Legacy,
    Advanced,
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to parse flag: {input}. Supported values are: legacy, advanced")]
pub struct InvalidSoftwareManagementApiFlag {
    input: String,
}

impl FromStr for SoftwareManagementApiFlag {
    type Err = InvalidSoftwareManagementApiFlag;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "legacy" => Ok(SoftwareManagementApiFlag::Legacy),
            "advanced" => Ok(SoftwareManagementApiFlag::Advanced),
            _ => Err(InvalidSoftwareManagementApiFlag {
                input: input.to_string(),
            }),
        }
    }
}
