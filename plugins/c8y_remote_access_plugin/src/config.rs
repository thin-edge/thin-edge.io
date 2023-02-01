use std::path::Path;
use std::path::PathBuf;

use miette::IntoDiagnostic;
use tedge_config::C8yUrlSetting;
use tedge_config::ConfigSettingAccessor;
use tedge_config::TEdgeConfig;

pub struct C8yUrl(pub String);

impl C8yUrl {
    pub fn retrieve(config: &TEdgeConfig) -> miette::Result<Self> {
        Ok(Self(
            config
                .query(C8yUrlSetting)
                .into_diagnostic()?
                .as_str()
                .to_owned(),
        ))
    }
}

pub fn supported_operation_path(config_dir: &Path) -> PathBuf {
    let mut path = config_dir.to_owned();
    path.push("operations/c8y/c8y_RemoteAccessConnect");
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_supported_operation_path() {
        assert_eq!(
            supported_operation_path("/etc/tedge".as_ref()),
            PathBuf::from("/etc/tedge/operations/c8y/c8y_RemoteAccessConnect")
        );
    }
}
