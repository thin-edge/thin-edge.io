#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    #[error("{context}")]
    Io {
        context: String,
        source: std::io::Error,
    },

    #[error("Could not make a successful request to the remote server")]
    Network(#[from] reqwest::Error),
}

pub(crate) trait ErrContext<T> {
    fn context(self, context: String) -> Result<T, UploadError>;
}

impl<T, E: Into<std::io::Error>> ErrContext<T> for Result<T, E> {
    fn context(self, context: String) -> Result<T, UploadError> {
        self.map_err(|err| UploadError::Io {
            context,
            source: err.into(),
        })
    }
}
