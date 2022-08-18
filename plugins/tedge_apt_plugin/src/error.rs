#[derive(thiserror::Error, Debug)]
pub enum InternalError {
    #[error("Fail to run `{cmd}`: {from}")]
    ExecError { cmd: String, from: std::io::Error },

    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    FromCsv(#[from] csv::Error),

    #[error("Parsing Debian package failed for `{file}`, Error: {error}")]
    ParsingError { file: String, error: String },

    #[error("Validation of {package} metadata failed, expected value for the {expected_key} is {expected_value}, but provided {provided_value}")]
    MetaDataMismatch {
        package: String,
        expected_key: String,
        expected_value: String,
        provided_value: String,
    },
}

impl InternalError {
    pub fn exec_error(cmd: impl Into<String>, from: std::io::Error) -> InternalError {
        InternalError::ExecError {
            cmd: cmd.into(),
            from,
        }
    }
}
