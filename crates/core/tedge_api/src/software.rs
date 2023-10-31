use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

use download::AnonymisedAuth;
use download::DownloadInfo;

pub type SoftwareType = String;
pub type SoftwareName = String;
pub type SoftwareVersion = String;

pub const DEFAULT: &str = "default";

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(bound(deserialize = "Auth: Deserialize<'de>"))]
pub struct SoftwareModule<Auth> {
    #[serde(default)]
    pub module_type: Option<SoftwareType>,
    pub name: SoftwareName,
    pub version: Option<SoftwareVersion>,
    pub url: Option<DownloadInfo<Auth>>,
    pub file_path: Option<PathBuf>,
}

impl<Auth> SoftwareModule<Auth> {
    pub fn default_type() -> SoftwareType {
        DEFAULT.to_string()
    }

    pub fn is_default_type(module_type: &str) -> bool {
        module_type.is_empty() || module_type == DEFAULT
    }

    pub fn new(
        module_type: Option<SoftwareType>,
        name: SoftwareName,
        version: Option<SoftwareVersion>,
        url: Option<DownloadInfo<Auth>>,
        file_path: Option<PathBuf>,
    ) -> Self {
        let mut module = SoftwareModule {
            module_type,
            name,
            version,
            url,
            file_path,
        };
        module.normalize();
        module
    }

    pub fn normalize(&mut self) {
        match &self.module_type {
            Some(module_type) if Self::is_default_type(module_type) => self.module_type = None,
            _ => {}
        };

        match &self.version {
            Some(version) if version.is_empty() => self.version = None,
            _ => {}
        };
    }
}

impl<Auth> SoftwareModule<Auth>
where
    for<'a> &'a Auth: Into<AnonymisedAuth>,
{
    pub fn clone_anonymise_auth(&self) -> SoftwareModule<AnonymisedAuth> {
        SoftwareModule {
            module_type: self.module_type.clone(),
            name: self.name.clone(),
            version: self.version.clone(),
            url: self.url.as_ref().map(|di| di.clone_anonymise_auth()),
            file_path: self.file_path.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
pub enum SoftwareModuleUpdate<Auth> {
    Install { module: SoftwareModule<Auth> },
    Remove { module: SoftwareModule<Auth> },
}

impl<Auth> SoftwareModuleUpdate<Auth> {
    pub fn install(module: SoftwareModule<Auth>) -> Self {
        SoftwareModuleUpdate::Install { module }
    }

    pub fn remove(module: SoftwareModule<Auth>) -> Self {
        SoftwareModuleUpdate::Remove { module }
    }

    pub fn module(&self) -> &SoftwareModule<Auth> {
        match self {
            SoftwareModuleUpdate::Install { module } | SoftwareModuleUpdate::Remove { module } => {
                module
            }
        }
    }

    pub fn normalize(&mut self) {
        let module = match self {
            SoftwareModuleUpdate::Install { module } | SoftwareModuleUpdate::Remove { module } => {
                module
            }
        };
        module.normalize();
    }
}
