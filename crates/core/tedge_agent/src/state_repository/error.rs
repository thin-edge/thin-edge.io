#[derive(Debug, thiserror::Error)]
#[allow(clippy::enum_variant_names)]
pub enum StateError {
    #[error(transparent)]
    FromTOMLParse(#[from] toml::de::Error),

    #[error(transparent)]
    FromInvalidTOML(#[from] toml::ser::Error),

    #[error(transparent)]
    FromIo(#[from] std::io::Error),
}
