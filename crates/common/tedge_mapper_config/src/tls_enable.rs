use std::fmt;
use std::str::FromStr;

use tedge_config_engine::AppendRemoveItem;

/// Whether the bridge connects to the cloud broker over TLS
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
#[facet(rename_all = "lowercase")]
pub enum TlsEnable {
    True,
    False,
    #[default]
    Auto,
}

impl fmt::Display for TlsEnable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::True => f.write_str("true"),
            Self::False => f.write_str("false"),
            Self::Auto => f.write_str("auto"),
        }
    }
}

impl FromStr for TlsEnable {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "true" => Ok(Self::True),
            "false" => Ok(Self::False),
            "auto" => Ok(Self::Auto),
            // The config engine lists the accepted values from the enum's variants
            _ => Err(format!("Unknown TLS setting '{s}'")),
        }
    }
}

impl AppendRemoveItem for TlsEnable {
    fn append(_current: Option<Self>, new_value: Self) -> Option<Self> {
        Some(new_value)
    }

    fn remove(current: Option<Self>, remove_value: Self) -> Option<Self> {
        match current {
            Some(v) if v == remove_value => None,
            other => other,
        }
    }
}
