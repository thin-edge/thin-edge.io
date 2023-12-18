use std::fmt::Display;
use std::fmt::Formatter;
use std::str::FromStr;

/// A flag that can be set to keep the old configs or to overwrite them,
/// meaning the system will have to detect the appropriate config setting
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Eq, PartialEq, doku::Document)]
pub enum AptConfig {
    KeepOld,
    KeepNew,
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to parse flag: {input}. Supported values are: keepold, keepnew")]
pub struct InvalidAptConfig {
    input: String,
}

impl FromStr for AptConfig {
    type Err = InvalidAptConfig;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "keepold" => Ok(AptConfig::KeepOld),
            "keepnew" => Ok(AptConfig::KeepNew),
            _ => Err(InvalidAptConfig {
                input: input.to_string(),
            }),
        }
    }
}

impl Display for AptConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            AptConfig::KeepOld => "keepold",
            AptConfig::KeepNew => "keepnew",
        };
        output.fmt(f)
    }
}
