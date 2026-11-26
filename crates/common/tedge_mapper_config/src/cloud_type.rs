use std::fmt;
use std::str::FromStr;

/// The built-in cloud integration a mapper uses
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
#[facet(rename_all = "lowercase")]
pub enum CloudType {
    C8y,
    Az,
    Aws,
}

impl fmt::Display for CloudType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::C8y => f.write_str("c8y"),
            Self::Az => f.write_str("az"),
            Self::Aws => f.write_str("aws"),
        }
    }
}

impl FromStr for CloudType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "c8y" => Ok(Self::C8y),
            "az" => Ok(Self::Az),
            "aws" => Ok(Self::Aws),
            // The config engine lists the accepted values from the enum's variants
            _ => Err(format!("Unknown cloud type '{s}'")),
        }
    }
}

tedge_config_engine::impl_single_value_append_remove!(CloudType);
