use std::fmt;
use std::str::FromStr;

/// When to upload an operation's log once it finishes
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
#[facet(rename_all = "kebab-case")]
pub enum AutoLogUpload {
    Never,
    Always,
    OnFailure,
}

impl fmt::Display for AutoLogUpload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Never => f.write_str("never"),
            Self::Always => f.write_str("always"),
            Self::OnFailure => f.write_str("on-failure"),
        }
    }
}

impl FromStr for AutoLogUpload {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "never" => Ok(Self::Never),
            "always" => Ok(Self::Always),
            "on-failure" => Ok(Self::OnFailure),
            _ => Err(format!(
                "Failed to parse flag: {input}. Supported values are: 'never', 'always' or 'on-failure'"
            )),
        }
    }
}

tedge_config_engine::impl_single_value_append_remove!(AutoLogUpload);
