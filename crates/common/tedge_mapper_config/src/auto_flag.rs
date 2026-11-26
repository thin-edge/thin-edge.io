use std::fmt;
use std::str::FromStr;

/// A flag that can be set to auto, meaning the system will have to detect the
/// appropriate true/false setting
///
/// `tedge config` takes and shows it lowercase, but tedge_config writes it to
/// files capitalised (`"Auto"`), so the variants keep their Rust names there
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum AutoFlag {
    True,
    False,
    Auto,
}

impl fmt::Display for AutoFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::True => f.write_str("true"),
            Self::False => f.write_str("false"),
            Self::Auto => f.write_str("auto"),
        }
    }
}

impl FromStr for AutoFlag {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "true" => Ok(Self::True),
            "false" => Ok(Self::False),
            "auto" => Ok(Self::Auto),
            _ => Err(format!(
                "Failed to parse flag: {s}. Supported values are: true, false, auto"
            )),
        }
    }
}

tedge_config_engine::impl_single_value_append_remove!(AutoFlag);
