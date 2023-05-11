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
    FromInvalidConfigUrl(#[from] crate::tedge_config_cli::models::InvalidConnectUrl),

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
