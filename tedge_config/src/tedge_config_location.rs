use std::path::PathBuf;

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
/// The choice, where we find `tedge.toml` OTOH is based on the executing user AND the env `$HOME`.
/// But once we have found `tedge.toml`, we never ever have to care about the executing user
/// (except when `chown`ing files...).
///
/// # NOTES
///
/// Why this is no trait? We need to `clone` a config location, and cloning a `Box<dyn>` is
/// difficult (you can use `dyn_clone` or have your own function `clone() -> Box<dyn ...>` of
/// course).
///
#[derive(Debug, Clone)]
pub enum TEdgeConfigLocation {
    /// `tedge.toml` is located in `/etc/tedge`. All defaults are based on system locations.
    SystemLocation { etc_path: PathBuf },

    /// `tedge.toml` is located in `$HOME/.tedge/tedge.toml`. All defaults are relative to the
    /// `$HOME/.tedge` directory.
    UserLocation { home_path: PathBuf },
}

impl TEdgeConfigLocation {
    pub fn default_system_location() -> Self {
        Self::SystemLocation {
            etc_path: DEFAULT_ETC_PATH.into(),
        }
    }
}

impl TEdgeConfigLocation {
    /// Full path to `tedge.toml`.
    pub(crate) fn tedge_config_path(&self) -> PathBuf {
        match self {
            Self::SystemLocation { etc_path } => etc_path.join("tedge").join(TEDGE_CONFIG_FILE),
            Self::UserLocation { home_path } => home_path.join(".tedge").join(TEDGE_CONFIG_FILE),
        }
    }

    /// Default device cert path
    pub(crate) fn default_device_cert_path(&self) -> PathBuf {
        match self {
            Self::SystemLocation { etc_path } => etc_path
                .join("ssl")
                .join("certs")
                .join("tedge-certificate.pem"),
            Self::UserLocation { home_path: _ } => unimplemented!(),
        }
    }

    /// Default device key path
    pub(crate) fn default_device_key_path(&self) -> PathBuf {
        match self {
            Self::SystemLocation { etc_path } => etc_path
                .join("ssl")
                .join("certs")
                .join("tedge-private-key.pem"),
            Self::UserLocation { home_path: _ } => unimplemented!(),
        }
    }

    /// Default path for azure root certificates
    pub(crate) fn default_azure_root_cert_path(&self) -> PathBuf {
        match self {
            Self::SystemLocation { etc_path } => etc_path.join("ssl").join("certs"),
            Self::UserLocation { home_path: _ } => unimplemented!(),
        }
    }

    /// Default path for c8y root certificates
    pub(crate) fn default_c8y_root_cert_path(&self) -> PathBuf {
        match self {
            Self::SystemLocation { etc_path } => etc_path.join("ssl").join("certs"),
            Self::UserLocation { home_path: _ } => unimplemented!(),
        }
    }
}
