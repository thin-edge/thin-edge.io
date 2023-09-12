use std::fmt::Display;
use std::str::FromStr;

use doku::Document;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Document)]
pub enum DeviceType {
    ThinEdgeDevice,
    ChildDevice,
}

impl Default for DeviceType {
    fn default() -> Self {
        Self::ThinEdgeDevice
    }
}

impl FromStr for DeviceType {
    type Err = DeviceTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "thin-edge.io" => Ok(Self::ThinEdgeDevice),
            "child-device" => Ok(Self::ChildDevice),
            _ => Err(DeviceTypeError),
        }
    }
}

impl DeviceType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ThinEdgeDevice => "thin-edge.io",
            Self::ChildDevice => "child-device",
        }
    }
}

impl Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(thiserror::Error, Debug, Clone, Copy)]
#[error("Provided string is not a valid device type")]
pub struct DeviceTypeError;
