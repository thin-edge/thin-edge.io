use serde::{Deserialize, Serialize};

pub type SoftwareType = String;
pub type SoftwareName = String;
pub type SoftwareVersion = String;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SoftwareModule {
    #[serde(default)]
    pub module_type: Option<SoftwareType>,
    pub name: SoftwareName,
    pub version: Option<SoftwareVersion>,
    pub url: Option<String>,
}

impl SoftwareModule {
    pub fn default_type() -> SoftwareType {
        "default".to_string()
    }

    pub fn is_default_type(module_type: &str) -> bool {
        module_type.is_empty() || module_type == "default"
    }

    pub fn new(
        module_type: Option<SoftwareType>,
        name: SoftwareName,
        version: Option<SoftwareVersion>,
        url: Option<String>,
    ) -> SoftwareModule {
        let module_type = match module_type {
            Some(module_type) if SoftwareModule::is_default_type(&module_type) => None,
            module_type => module_type,
        };

        let version = match version {
            Some(version) if version.is_empty() => None,
            version => version,
        };

        SoftwareModule {
            module_type,
            name,
            version,
            url,
        }
    }
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

    pub fn module(&self) -> &SoftwareModule {
        match self {
            SoftwareModuleUpdate::Install { module } |
            SoftwareModuleUpdate::Remove { module } => module
        }
    }
}
