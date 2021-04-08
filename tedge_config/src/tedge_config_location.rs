use std::path::{Path, PathBuf};

const DEFAULT_ETC_PATH: &str = "/etc";
const TEDGE_CONFIG_FILE: &str = "tedge.toml";

/// Information about where `tedge.toml` is located and the defaults that are based
/// on that location.
///
/// Broadly speaking, we distinguish two different locations:
///
/// - System-wide locations under `/etc/tedge`
/// - User-local locations under `$HOME/.tedge`
///
/// We DO NOT base the defaults on the currently executing user. Instead, we base
/// the defaults on the location of the `tedge.toml` file. If it is located in
/// `/etc`, regardless of the executing user, we use defaults that use system-wide
/// locations (e.g. `/etc/ssl/certs`). Whereas if `tedge.toml` is located in a users
/// home directory, we base the defaults on locations within the users home directory.
///
/// This allows run `sudo tedge -c '$HOME/.tedge/tedge.toml ...` where the defaults are picked up
/// correctly.
///
/// The choice, where to find `tedge.toml` on the other hand is based on the executing user AND the
/// env `$HOME`.  But once we have found `tedge.toml`, we never again have to care about the
/// executing user (except when `chown`ing files...).
///
#[derive(Debug, Clone)]
pub struct TEdgeConfigLocation {
    /// Full path to `tedge.toml`.
    pub tedge_config_path: PathBuf,

    /// Default device cert path
    pub default_device_cert_path: PathBuf,

    /// Default device key path
    pub default_device_key_path: PathBuf,

    /// Default path for azure root certificates
    pub default_azure_root_cert_path: PathBuf,

    /// Default path for c8y root certificates
    pub default_c8y_root_cert_path: PathBuf,
}

impl TEdgeConfigLocation {
    /// `tedge.toml` is located in `/etc/tedge`. All defaults are based on system locations.
    pub fn from_default_system_location() -> Self {
        Self::from_system_location(DEFAULT_ETC_PATH)
    }

    /// `tedge.toml` is located in `${etc_path}/tedge`. All defaults are based on system locations.
    pub fn from_system_location(etc_path: impl AsRef<Path>) -> Self {
        let etc_path = etc_path.as_ref();
        let tedge_path = etc_path.join("tedge");
        Self {
            tedge_config_path: tedge_path.join(TEDGE_CONFIG_FILE),
            default_device_cert_path: tedge_path
                .join("device-certs")
                .join("tedge-certificate.pem"),
            default_device_key_path: tedge_path
                .join("device-certs")
                .join("tedge-private-key.pem"),
            default_azure_root_cert_path: etc_path.join("ssl").join("certs"),
            default_c8y_root_cert_path: etc_path.join("ssl").join("certs"),
        }
    }

    /// `tedge.toml` is located in `$HOME/.tedge/tedge.toml`. All defaults are relative to the
    /// `$HOME/.tedge` directory.
    pub fn from_users_home_location(home_path: impl AsRef<Path>) -> Self {
        let etc_path = Path::new(DEFAULT_ETC_PATH);
        let tedge_path = home_path.as_ref().join(".tedge");
        Self {
            tedge_config_path: tedge_path.join(TEDGE_CONFIG_FILE),
            default_device_cert_path: tedge_path
                .join("device-certs")
                .join("tedge-certificate.pem"),
            default_device_key_path: tedge_path
                .join("device-certs")
                .join("tedge-private-key.pem"),
            default_azure_root_cert_path: etc_path.join("ssl").join("certs"),
            default_c8y_root_cert_path: etc_path.join("ssl").join("certs"),
        }
    }
}

#[test]
fn test_from_default_system_location() {
    let config_location = TEdgeConfigLocation::from_default_system_location();
    assert_eq!(
        config_location.tedge_config_path,
        PathBuf::from("/etc/tedge/tedge.toml")
    );
    assert_eq!(
        config_location.default_device_cert_path,
        PathBuf::from("/etc/tedge/device-certs/tedge-certificate.pem")
    );
    assert_eq!(
        config_location.default_device_key_path,
        PathBuf::from("/etc/tedge/device-certs/tedge-private-key.pem")
    );
    assert_eq!(
        config_location.default_azure_root_cert_path,
        PathBuf::from("/etc/ssl/certs")
    );
    assert_eq!(
        config_location.default_c8y_root_cert_path,
        PathBuf::from("/etc/ssl/certs")
    );
}

#[test]
fn test_from_system_location() {
    // "/usr/local/etc" is often used for installed services on FreeBSD
    let config_location = TEdgeConfigLocation::from_system_location("/usr/local/etc");
    assert_eq!(
        config_location.tedge_config_path,
        PathBuf::from("/usr/local/etc/tedge/tedge.toml")
    );
    assert_eq!(
        config_location.default_device_cert_path,
        PathBuf::from("/usr/local/etc/tedge/device-certs/tedge-certificate.pem")
    );
    assert_eq!(
        config_location.default_device_key_path,
        PathBuf::from("/usr/local/etc/tedge/device-certs/tedge-private-key.pem")
    );
    // XXX: This should actually be "/etc/ssl/certs".
    assert_eq!(
        config_location.default_azure_root_cert_path,
        PathBuf::from("/usr/local/etc/ssl/certs")
    );
    // XXX: This should actually be "/etc/ssl/certs".
    assert_eq!(
        config_location.default_c8y_root_cert_path,
        PathBuf::from("/usr/local/etc/ssl/certs")
    );
}

#[test]
fn test_from_users_home_location() {
    let config_location = TEdgeConfigLocation::from_users_home_location("/home/user");
    assert_eq!(
        config_location.tedge_config_path,
        PathBuf::from("/home/user/.tedge/tedge.toml")
    );
    assert_eq!(
        config_location.default_device_cert_path,
        PathBuf::from("/home/user/.tedge/device-certs/tedge-certificate.pem")
    );
    assert_eq!(
        config_location.default_device_key_path,
        PathBuf::from("/home/user/.tedge/device-certs/tedge-private-key.pem")
    );
    assert_eq!(
        config_location.default_azure_root_cert_path,
        PathBuf::from("/etc/ssl/certs")
    );
    assert_eq!(
        config_location.default_c8y_root_cert_path,
        PathBuf::from("/etc/ssl/certs")
    );
}
