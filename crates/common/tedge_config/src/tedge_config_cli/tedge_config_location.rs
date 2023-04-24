use std::path::Path;

use camino::Utf8Path;
use camino::Utf8PathBuf;

pub const DEFAULT_TEDGE_CONFIG_PATH: &str = "/etc/tedge";
const TEDGE_CONFIG_FILE: &str = "tedge.toml";
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
    pub tedge_config_root_path: Utf8PathBuf,

    /// Full path to the `tedge.toml` file.
    pub tedge_config_file_path: Utf8PathBuf,
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
            tedge_config_root_path: Utf8Path::from_path(tedge_config_root_path.as_ref())
                .unwrap()
                .to_owned(),
            tedge_config_file_path: Utf8Path::from_path(tedge_config_root_path.as_ref())
                .unwrap()
                .join(TEDGE_CONFIG_FILE),
        }
    }

    pub fn tedge_config_root_path(&self) -> &Utf8Path {
        &self.tedge_config_root_path
    }

    pub fn tedge_config_file_path(&self) -> &Utf8Path {
        &self.tedge_config_file_path
    }
}

#[test]
fn test_from_custom_root() {
    let config_location = TEdgeConfigLocation::from_custom_root("/opt/etc/tedge");
    assert_eq!(
        config_location.tedge_config_root_path,
        Utf8Path::new("/opt/etc/tedge")
    );
    assert_eq!(
        config_location.tedge_config_file_path,
        Utf8Path::new("/opt/etc/tedge/tedge.toml")
    );
}

#[test]
fn test_from_default_system_location() {
    let config_location = TEdgeConfigLocation::default();
    assert_eq!(
        config_location.tedge_config_root_path,
        Utf8Path::new("/etc/tedge")
    );
    assert_eq!(
        config_location.tedge_config_file_path,
        Utf8Path::new("/etc/tedge/tedge.toml")
    );
}
