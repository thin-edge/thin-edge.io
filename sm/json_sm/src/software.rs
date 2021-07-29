use crate::error::SoftwareError;
use serde::{Deserialize, Serialize};

pub type SoftwareType = String;
pub type SoftwareName = String;
pub type SoftwareVersion = String;


#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SoftwareModule {
    pub name: SoftwareName,
    pub version: Option<SoftwareVersion>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum SoftwareModuleUpdate {
    Install { module: SoftwareModule },
    Remove { module: SoftwareModule },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SoftwareModuleUpdateResult {
    pub update: SoftwareModuleUpdate,
    pub error: Option<SoftwareError>,
}
