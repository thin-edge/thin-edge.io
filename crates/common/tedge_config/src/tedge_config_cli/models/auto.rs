use std::fmt::Display;
use std::fmt::Formatter;
use std::str::FromStr;

/// A flag that can be set to auto,
/// meaning the system will have to detect the appropriate true/false setting
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Eq, PartialEq, doku::Document)]
pub enum AutoFlag {
    True,
    False,
    Auto,
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to parse flag: {input}. Supported values are: true, false, auto")]
pub struct InvalidAutoFlag {
    input: String,
}

impl FromStr for AutoFlag {
    type Err = InvalidAutoFlag;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "true" => Ok(AutoFlag::True),
            "false" => Ok(AutoFlag::False),
            "auto" => Ok(AutoFlag::Auto),
            _ => Err(InvalidAutoFlag {
                input: input.to_string(),
            }),
        }
    }
}

impl Display for AutoFlag {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            AutoFlag::True => "true",
            AutoFlag::False => "false",
            AutoFlag::Auto => "auto",
        };
        output.fmt(f)
    }
}
