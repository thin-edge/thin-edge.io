#[derive(thiserror::Error, Debug)]
pub enum InternalError {
    #[error("Fail to run `{cmd}`: {from}")]
    ExecError { cmd: String, from: std::io::Error },

    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error("Parsing Debian package failed for `{file}`")]
    ParsingError { file: String },

    #[error(transparent)]
    FromCsv(#[from] csv::Error),

    #[error("Removal of {} failed with version mismatch. Installed version: {installed}, Requested version: {requested}")]
    VersionMismatch {
        package: String,
        installed: String,
        requested: String,
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
