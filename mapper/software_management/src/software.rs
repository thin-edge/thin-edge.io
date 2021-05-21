use serde::{Deserialize, Serialize};

pub type SoftwareType = String;
pub type SoftwareName = String;
pub type SoftwareVersion = String;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SoftwareModule {
    pub software_type: SoftwareType,
    pub name: SoftwareName,
    pub version: Option<SoftwareVersion>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum SoftwareOperation {
    // A request for the current software list
    CurrentSoftwareList,

    // A sequence of updates to be applied
    SoftwareUpdates { updates: Vec<SoftwareUpdate> },

    // The desired software list
    DesiredSoftwareList { modules: Vec<SoftwareModule> },
}

#[derive(Debug, Deserialize, Serialize)]
pub enum SoftwareUpdate {
    Install { module: SoftwareModule },
    UnInstall { module: SoftwareModule },
}

#[derive(Debug, Deserialize, Serialize)]
pub enum SoftwareOperationStatus {
    SoftwareUpdates { updates: Vec<SoftwareUpdateStatus> },
    DesiredSoftwareList { updates: Vec<SoftwareUpdateStatus> },
    CurrentSoftwareList { modules: Vec<SoftwareModule> },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SoftwareUpdateStatus {
    update: SoftwareUpdate,
    status: UpdateStatus,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum UpdateStatus {
    Scheduled,
    Success,
    Error { reason: SoftwareError },
    Cancelled,
}

#[derive(thiserror::Error, Debug, Deserialize, Serialize)]
pub enum SoftwareError {
    #[error("Unknown software type: {software_type:?}")]
    UnknownSoftwareType { software_type: SoftwareType },

    #[error("Unknown {software_type:?} module: {name:?}")]
    UnknownModule {
        software_type: SoftwareType,
        name: SoftwareName,
    },

    #[error("Unknown {software_type:?} version: {name:?} - {version:?}")]
    UnknownVersion {
        software_type: SoftwareType,
        name: SoftwareName,
        version: SoftwareVersion,
    },

    #[error("Unexpected module type: actual: {actual_type:?}, expected: {expected_type:?}")]
    WrongModuleType {
        actual_type: SoftwareType,
        expected_type: SoftwareType,
    },

    #[error("Plugin error for {software_type:?}, reason: {reason:?}")]
    PluginError {
        software_type: SoftwareType,
        reason: String,
    },

    #[error("Fail to install {module:?}")]
    InstallError {
        module: SoftwareModule,
        reason: String,
    },

    #[error("Fail to uninstall {module:?}")]
    UnInstallError {
        module: SoftwareModule,
        reason: String,
    },
}

pub trait SoftwareListConsumer {
    type Outcome;
    type Error: std::error::Error;

    fn start(&mut self) -> Result<(), Self::Error>;
    fn add_module(&mut self, module: &SoftwareModule) -> Result<(), Self::Error>;
    fn finalize(&mut self) -> Result<Self::Outcome, Self::Error>;
}

pub trait SoftwareListProducer {
    fn produce<C, O, E>(&self, consumer: &mut C) -> Result<O, E>
    where
        C: SoftwareListConsumer<Outcome = O, Error = E>,
        E: std::error::Error;
}

impl SoftwareListProducer for () {
    fn produce<C, O, E>(&self, consumer: &mut C) -> Result<O, E>
    where
        C: SoftwareListConsumer<Outcome = O, Error = E>,
        E: std::error::Error,
    {
        let () = consumer.start()?;
        Ok(consumer.finalize()?)
    }
}
