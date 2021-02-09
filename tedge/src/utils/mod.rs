pub mod files;
pub mod services;

#[derive(thiserror::Error, Debug)]
pub enum UtilsError {
    #[error("Bridge connection has not been established, check configuration and try again.")]
    BridgeConnectionFailed,

    #[error("Connection cannot be established as config already exists. Please remove existing configuration for the bridge and try again.")]
    ConfigurationExists,

    #[error("Couldn't set MQTT Server to start on boot.")]
    MosquittoCantPersist,

    #[error("MQTT Server is active on the system as a service, please stop the service before you use this command.")]
    MosquittoIsActive,

    #[error("MQTT Server is not available on the system, it is required to use this command.")]
    MosquittoNotAvailable,

    #[error("MQTT Server is not available on the system as a service, it is required to use this command.")]
    MosquittoNotAvailableAsService,

    #[error("IO Error.")]
    StdIoError(#[from] std::io::Error),

    #[error("Couldn't find path to 'sudo'.")]
    SudoNotFound(#[from] which::Error),

    #[error("Systemd is not available on the system or elevated permissions have not been granted, it is required to use this command.")]
    SystemdNotAvailable,

    #[error("Returned error is not recognised: {code:?}.")]
    UnknownReturnCode { code: Option<i32> },
}
