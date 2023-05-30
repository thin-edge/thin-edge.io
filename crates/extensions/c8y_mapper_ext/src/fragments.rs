use crate::error::ConversionError;
use serde::Serialize;

const DEFAULT_AGENT_FRAGMENT_NAME: &str = "thin-edge.io";
const DEFAULT_AGENT_FRAGMENT_URL: &str = "https://thin-edge.io";

#[derive(Debug, Serialize)]
pub struct C8yAgent {
    name: String,
    version: String,
    url: String,
}

#[derive(Debug, Serialize)]
pub struct C8yAgentFragment {
    #[serde(rename = "c8y_Agent")]
    pub c8y_agent: C8yAgent,
}

impl C8yAgentFragment {
    pub fn new() -> Result<Self, ConversionError> {
        let c8y_agent = C8yAgent {
            name: DEFAULT_AGENT_FRAGMENT_NAME.into(),
            version: get_tedge_version(),
            url: DEFAULT_AGENT_FRAGMENT_URL.into(),
        };
        Ok(Self { c8y_agent })
    }

    pub fn to_json(&self) -> Result<serde_json::Value, ConversionError> {
        let json_string = serde_json::to_string(&self)?;
        let jsond: serde_json::Value = serde_json::from_str(&json_string)?;
        Ok(jsond)
    }
}

pub fn get_tedge_version() -> String {
    // Use package version over tedge cli to remove dependency on the optional tedge cli #1991
    env!("CARGO_PKG_VERSION").to_string()
}

#[derive(Debug, Serialize)]
pub struct C8yDeviceDataFragment {
    #[serde(rename = "type")]
    device_type: String,
}

impl C8yDeviceDataFragment {
    pub fn from_type(device_type: &str) -> Result<Self, ConversionError> {
        Ok(Self {
            device_type: device_type.into(),
        })
    }

    pub fn to_json(&self) -> Result<serde_json::Value, ConversionError> {
        let json_string = serde_json::to_string(&self)?;
        let jsond: serde_json::Value = serde_json::from_str(&json_string)?;
        Ok(jsond)
    }
}
