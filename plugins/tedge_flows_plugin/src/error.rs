use camino::Utf8Path;
use camino::Utf8PathBuf;
use thiserror::Error;

pub fn io_error(path: impl AsRef<Utf8Path>, error: std::io::Error) -> FlowsPluginError {
    FlowsPluginError::IoError {
        path: path.as_ref().to_path_buf(),
        error,
    }
}

#[derive(Error, Debug)]
pub enum FlowsPluginError {
    #[error("Invalid usage")]
    InvalidUsage,

    #[error("Could not access {path}: {error}")]
    IoError {
        path: Utf8PathBuf,
        error: std::io::Error,
    },

    #[error("Failed to parse flow.toml at {path}: {source}")]
    ParseFlowTomlError {
        path: Utf8PathBuf,
        source: toml::de::Error,
    },

    #[error("Invalid module name '{0}': expected '<mapper>/<flow-name>' with no path traversal (no '..', '.', empty segments, or leading '/').")]
    InvalidModuleName(String),

    #[error("Unsupported format for '{0}'")]
    UnsupportedFormat(String),

    #[error("Failed to unpack flow archive to {path}: {error}")]
    UnpackError {
        path: Utf8PathBuf,
        error: std::io::Error,
    },

    #[error("Provided flow archive is invalid. See output:\n{stderr}")]
    InvalidFlow { stderr: String },
}
