use std::fmt::Display;
use std::fmt::Formatter;
use std::str::FromStr;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Eq, PartialEq, doku::Document)]
#[serde(rename_all = "lowercase")]
pub enum Cryptoki {
    Off,
    Socket,
    Module,
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to parse flag: {input}. Supported values are: off, module, socket")]
pub struct InvalidCryptoki {
    input: String,
}

impl FromStr for Cryptoki {
    type Err = InvalidCryptoki;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "off" => Ok(Cryptoki::Off),
            "socket" => Ok(Cryptoki::Socket),
            "module" => Ok(Cryptoki::Module),
            _ => Err(InvalidCryptoki {
                input: input.to_string(),
            }),
        }
    }
}

impl Display for Cryptoki {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            Cryptoki::Off => "off",
            Cryptoki::Socket => "socket",
            Cryptoki::Module => "module",
        };
        output.fmt(f)
    }
}
