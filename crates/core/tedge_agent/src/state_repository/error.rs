use camino::Utf8Path;

#[derive(Debug, thiserror::Error)]
#[allow(clippy::enum_variant_names)]
pub enum StateError {
    #[error("State file `{path}` contains invalid syntax: {source}")]
    FromTOMLParse {
        path: Box<Utf8Path>,
        source: toml::de::Error,
    },

    #[error(transparent)]
    FromInvalidTOML(#[from] toml::ser::Error),

    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error("Could not read state from file `{path}`: {source}")]
    LoadingFromFileFailed {
        path: Box<Utf8Path>,
        source: std::io::Error,
    },

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
}
