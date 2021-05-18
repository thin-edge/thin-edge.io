use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct SoftwareList {
    pub modules: Vec<SoftwareModule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SoftwareModule {
    pub software_type: String,
    pub name: String,
    pub version: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum SoftwareOperation {
    SoftwareUpdate { updates: Vec<SoftwareUpdate> },
    SoftwareList { module_type: Option<String> },
}

#[derive(Debug, Deserialize, Serialize)]
pub enum SoftwareUpdate {
    Install { module: SoftwareModule },
    UnInstall { module: SoftwareModule },
}

#[derive(thiserror::Error, Debug, Deserialize, Serialize)]
pub enum SoftwareError {
    #[error("Unknown software type: {software_type:?}")]
    UnknownSoftwareType { software_type: String },

    #[error("Unknown {software_type:?} module: {name:?}")]
    UnknownModule { software_type: String, name: String },

    #[error("Unknown {software_type:?} version: {name:?} - {version:?}")]
    UnknownVersion {
        software_type: String,
        name: String,
        version: String,
    },

    #[error("Unexpected module type: actual: {actual_type:?}, expected: {expected_type:?}")]
    WrongModuleType {
        actual_type: String,
        expected_type: String,
    },

    #[error("Plugin error for {software_type:?}, reason: {reason:?}")]
    PluginError {
        software_type: String,
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
