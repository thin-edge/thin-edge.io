use std::fmt::Display;
use std::fmt::Formatter;
use std::str::FromStr;

/// The protocol used to connect to a cloud's MQTT service
#[derive(
    Debug, Clone, Copy, serde::Serialize, serde::Deserialize, Eq, PartialEq, doku::Document,
)]
pub enum MqttProtocol {
    Tcp,
    Websocket,
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to parse flag: {input}. Supported values are: tcp, ws")]
pub struct InvalidScheme {
    input: String,
}

impl FromStr for MqttProtocol {
    type Err = InvalidScheme;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "tcp" => Ok(Self::Tcp),
            "ws" => Ok(Self::Websocket),
            _ => Err(Self::Err {
                input: input.to_string(),
            }),
        }
    }
}

impl Display for MqttProtocol {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            Self::Tcp => "tcp",
            Self::Websocket => "ws",
        };
        output.fmt(f)
    }
}
