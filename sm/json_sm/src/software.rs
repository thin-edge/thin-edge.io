use serde::{Deserialize, Serialize};

pub type SoftwareType = String;
pub type SoftwareName = String;
pub type SoftwareVersion = String;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SoftwareModule {
    #[serde(default)]
    pub module_type: SoftwareType,
    pub name: SoftwareName,
    pub version: Option<SoftwareVersion>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum SoftwareModuleUpdate {
    Install { module: SoftwareModule },
    Remove { module: SoftwareModule },
}

impl SoftwareModuleUpdate {
    pub fn install(module: SoftwareModule) -> SoftwareModuleUpdate {
        SoftwareModuleUpdate::Install { module }
    }

    pub fn remove(module: SoftwareModule) -> SoftwareModuleUpdate {
        SoftwareModuleUpdate::Remove { module }
    }
}
