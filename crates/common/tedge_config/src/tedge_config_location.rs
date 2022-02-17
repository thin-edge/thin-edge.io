use std::path::{Path, PathBuf};

pub const DEFAULT_TEDGE_CONFIG_PATH: &str = "/etc/tedge";
const TEDGE_CONFIG_FILE: &str = "tedge.toml";
const TEDGE_CONFIG_FILE_TMP: &str = "tedge.toml.tmp";

/// Information about where `tedge.toml` is located.
///
/// Broadly speaking, we distinguish two different locations:
///
/// - System-wide locations under `/etc/tedge` or `/usr/local/etc/tedge`.
/// - User-local locations under `$HOME/.tedge`
///
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TEdgeConfigLocation {
    /// Root directory where `tedge.toml` and other tedge related configuration files are located.
    pub tedge_config_root_path: PathBuf,

    /// Full path to the `tedge.toml` file.
    pub tedge_config_file_path: PathBuf,
}

impl Default for TEdgeConfigLocation {
    /// `tedge.toml` is located in `/etc/tedge`.
    fn default() -> Self {
        Self::from_custom_root(DEFAULT_TEDGE_CONFIG_PATH)
    }
}

impl TEdgeConfigLocation {
    pub fn from_custom_root(tedge_config_root_path: impl AsRef<Path>) -> Self {
        Self {
            tedge_config_root_path: tedge_config_root_path.as_ref().to_path_buf(),
            tedge_config_file_path: tedge_config_root_path.as_ref().join(TEDGE_CONFIG_FILE),
        }
    }

    pub fn tedge_config_root_path(&self) -> &Path {
        &self.tedge_config_root_path
    }
    pub fn tedge_config_file_path(&self) -> &Path {
        &self.tedge_config_file_path
    }

    pub fn temporary_tedge_config_file_path(&self) -> impl AsRef<Path> {
        self.tedge_config_root_path.join(TEDGE_CONFIG_FILE_TMP)
    }
}

#[test]
fn test_from_custom_root() {
    let config_location = TEdgeConfigLocation::from_custom_root("/opt/etc/tedge");
    assert_eq!(
        config_location.tedge_config_root_path,
        PathBuf::from("/opt/etc/tedge")
    );
    assert_eq!(
        config_location.tedge_config_file_path,
        PathBuf::from("/opt/etc/tedge/tedge.toml")
    );
}

#[test]
fn test_from_default_system_location() {
    let config_location = TEdgeConfigLocation::default();
    assert_eq!(
        config_location.tedge_config_root_path,
        PathBuf::from("/etc/tedge")
    );
    assert_eq!(
        config_location.tedge_config_file_path,
        PathBuf::from("/etc/tedge/tedge.toml")
    );
}
