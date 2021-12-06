use std::process::ExitStatus;

#[derive(thiserror::Error, Debug)]
pub enum InternalError {
    #[error("Fail to run `{cmd}`: {from}")]
    ExecError { cmd: String, from: std::io::Error },

    #[error("Execution of `{cmd}` failed with exit status {exit_status}")]
    ExecFailure {
        cmd: String,
        exit_status: ExitStatus,
    },

    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    FromZipError(#[from] zip::result::ZipError),

    #[error(transparent)]
    FromXmlError(#[from] roxmltree::Error),

    #[error("Apama not installed at /opt/softwareag/Apama")]
    ApamaNotInstalled,

    #[error("Module type with suffix ::{module_type} is not supported")]
    UnsupportedModuleType { module_type: String },

    #[error(
        "Module type suffix not provided in module name: `{module_name}`. Add ::project or ::mon"
    )]
    ModuleTypeNotProvided { module_name: String },
}

impl InternalError {
    pub fn exec_error(cmd: impl Into<String>, from: std::io::Error) -> InternalError {
        InternalError::ExecError {
            cmd: cmd.into(),
            from,
        }
    }
}
