#[derive(thiserror::Error, Debug)]
pub enum TEdgeConfigError {
    #[error("TOML parse error")]
    FromTOMLParse(#[from] toml::de::Error),

    #[error("TOML serialization error")]
    FromInvalidTOML(#[from] toml::ser::Error),

    #[error("I/O error")]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromConfigSetting(#[from] crate::ConfigSettingError),

    #[error(transparent)]
    FromInvalidConfigUrl(#[from] crate::tedge_toml::models::InvalidConnectUrl),

    #[error("Config file not found: {0}")]
    ConfigFileNotFound(std::path::PathBuf),

    #[error("Home directory is not found.")]
    HomeDirNotFound,

    #[error(transparent)]
    Figment(#[from] figment::Error),

    #[error(transparent)]
    Multi(#[from] Multi),

    #[error(transparent)]
    DirNotFound(#[from] tedge_utils::paths::PathsError),

    #[error(transparent)]
    FromParseHostPortError(#[from] crate::tedge_toml::models::host_port::ParseHostPortError),

    #[error(transparent)]
    FromAtomFileError(#[from] tedge_utils::fs::AtomFileError),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

pub type ConfigSettingResult<T> = Result<T, ConfigSettingError>;

#[derive(thiserror::Error, Debug)]
/// An error encountered while updating a value in tedge.toml
pub enum ConfigSettingError {
    #[error(
        r#"A value for `{key}` is missing.
    A value can be set with `tedge config set {key} <value>`"#
    )]
    ConfigNotSet { key: &'static str },

    #[error("Readonly setting: {message}")]
    ReadonlySetting { message: &'static str },

    #[error("Conversion from String failed")]
    ConversionFromStringFailed,

    #[error("Conversion into String failed")]
    ConversionIntoStringFailed,

    #[error("Derivation for `{key}` failed: {cause}")]
    DerivationFailed { key: &'static str, cause: String },

    #[error("Config value {key}, cannot be configured: {message} ")]
    SettingIsNotConfigurable {
        key: &'static str,
        message: &'static str,
    },

    #[error("An error occurred: {msg}")]
    Other { msg: &'static str },

    #[error(transparent)]
    Write(#[from] super::WriteError),
}

impl TEdgeConfigError {
    pub fn multiple_errors(mut errors: Vec<Self>) -> Self {
        match errors.len() {
            1 => errors.remove(0),
            _ => Self::Multi(Multi(errors)),
        }
    }
}

#[derive(Debug)]
pub struct Multi(Vec<TEdgeConfigError>);

impl std::fmt::Display for Multi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for error in &self.0 {
            if !first {
                writeln!(f)?;
            }

            write!(f, "{error}")?;

            first = false;
        }

        Ok(())
    }
}

impl std::error::Error for Multi {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.first()?.source()
    }
}
