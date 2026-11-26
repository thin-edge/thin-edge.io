use std::fmt;
use std::str::FromStr;

/// Whether and how a Hardware Security Module signs for the cloud connection
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
#[facet(rename_all = "lowercase")]
pub enum Cryptoki {
    Off,
    Socket,
    Module,
}

impl fmt::Display for Cryptoki {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Off => f.write_str("off"),
            Self::Socket => f.write_str("socket"),
            Self::Module => f.write_str("module"),
        }
    }
}

impl FromStr for Cryptoki {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "off" => Ok(Self::Off),
            "socket" => Ok(Self::Socket),
            "module" => Ok(Self::Module),
            // The config engine lists the accepted values from the enum's variants
            _ => Err(format!("Unknown cryptoki mode '{s}'")),
        }
    }
}

tedge_config_engine::impl_single_value_append_remove!(Cryptoki);
