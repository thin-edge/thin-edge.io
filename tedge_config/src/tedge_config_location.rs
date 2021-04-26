use std::path::{Path, PathBuf};

const DEFAULT_ETC_PATH: &str = "/etc";
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
    tedge_config_root_path: PathBuf,
}

impl TEdgeConfigLocation {
    pub fn from_custom_root(tedge_config_root_path: impl AsRef<Path>) -> Self {
        Self {
            tedge_config_root_path: tedge_config_root_path.as_ref().to_path_buf(),
        }
    }

    /// `tedge.toml` is located in `/etc/tedge`.
    pub fn from_default_system_location() -> Self {
        Self::from_custom_root(Path::new(DEFAULT_ETC_PATH).join("tedge"))
    }

    /// `tedge.toml` is located in `${etc_path}/tedge`.
    pub fn from_custom_etc_location(custom_etc_path: impl AsRef<Path>) -> Self {
        Self::from_custom_root(custom_etc_path.as_ref().join("tedge"))
    }

    /// `tedge.toml` is located in `${home_path}/.tedge`.
    pub fn from_users_home_location(home_path: impl AsRef<Path>) -> Self {
        Self::from_custom_root(home_path.as_ref().join(".tedge"))
    }

    pub fn tedge_config_root_path(&self) -> &Path {
        &self.tedge_config_root_path
    }

    pub fn tedge_config_file_path(&self) -> PathBuf {
        self.tedge_config_root_path.join(TEDGE_CONFIG_FILE)
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
        config_location.tedge_config_file_path(),
        PathBuf::from("/opt/etc/tedge/tedge.toml")
    );
}

#[test]
fn test_from_default_system_location() {
    let config_location = TEdgeConfigLocation::from_default_system_location();
    assert_eq!(
        config_location.tedge_config_root_path,
        PathBuf::from("/etc/tedge")
    );
    assert_eq!(
        config_location.tedge_config_file_path(),
        PathBuf::from("/etc/tedge/tedge.toml")
    );
}

#[test]
fn test_from_custom_etc_location() {
    let config_location = TEdgeConfigLocation::from_custom_etc_location("/usr/local/etc");
    assert_eq!(
        config_location.tedge_config_root_path(),
        PathBuf::from("/usr/local/etc/tedge")
    );
    assert_eq!(
        config_location.tedge_config_file_path(),
        PathBuf::from("/usr/local/etc/tedge/tedge.toml")
    );
}

#[test]
fn test_from_users_home_location() {
    let config_location = TEdgeConfigLocation::from_users_home_location("/home/user");
    assert_eq!(
        config_location.tedge_config_root_path(),
        PathBuf::from("/home/user/.tedge")
    );
    assert_eq!(
        config_location.tedge_config_file_path(),
        PathBuf::from("/home/user/.tedge/tedge.toml")
    );
}
