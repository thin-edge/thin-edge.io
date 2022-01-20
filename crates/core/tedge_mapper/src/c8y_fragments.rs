use serde::Serialize;
use std::process::Command;

use crate::error::ConversionError;
use tracing::error;

// rename to c8y_agent_fragment.rs

#[derive(Debug, Serialize)]
pub struct C8yAgentFragment {
    name: String,
    version: String,
}

impl C8yAgentFragment {
    pub fn new() -> Result<Self, ConversionError> {
        Ok(Self {
            name: "thin-edge.io".to_string(),
            version: get_tedge_version()?,
        })
    }

    pub fn with_name(&self, name: &str) -> Result<Self, ConversionError> {
        Ok(Self {
            name: name.to_string(),
            version: self.version.clone(),
        })
    }

    pub fn to_json(&self) -> Result<serde_json::Value, ConversionError> {
        let json_string = serde_json::to_string(&self)?;
        let jsond: serde_json::Value = serde_json::from_str(&json_string)?;
        Ok(jsond)
    }
}
pub fn get_tedge_version() -> Result<String, ConversionError> {
    let process = Command::new("tedge").arg("--version").output();

    match process {
        Ok(process) => {
            let string = String::from_utf8(process.stdout)?;
            Ok(string
                .split_whitespace()
                .last()
                .ok_or_else(|| ConversionError::FromOptionError)?
                .trim()
                .to_string())
        }
        Err(err) => {
            error!("{}", err);
            Ok("0.0.0".to_string())
        }
    }
}
