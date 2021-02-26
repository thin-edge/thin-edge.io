use crate::command::{BuildCommand, Command};
use structopt::StructOpt;

use crate::utils::{paths,services};
use crate::config::ConfigError;
use tempfile::PersistError;

mod c8y;
mod az;

#[derive(StructOpt, Debug)]
pub enum ConnectCmd {
    /// Create connection to Cumulocity
    ///
    /// The command will create config and start edge relay from the device to c8y instance
    C8y(c8y::Connect),

    /// Create connection to Azure 
    ///
    /// The command will create config and start edge relay from the device to az instance
    AZ(az::Connect),
}

impl BuildCommand for ConnectCmd {
    fn build_command(
        self,
        config: crate::config::TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::config::ConfigError> {
        match self {
            ConnectCmd::C8y(cmd) => cmd,
            ConnectCmd::AZ(cmd) => cmd,
        }
    }
}

impl Command for ConnectCmd {
    fn to_string(&self) -> String {
        self.sub_command().to_string()
    }

    fn run(&self, verbose: u8) -> Result<(), anyhow::Error> {
        self.sub_command().run(verbose)
    }
}

#[derive(thiserror::Error, Debug)]
enum ConnectError {
    #[error("Bridge has been configured, but {cloud} connection check failed.")]
    BridgeConnectionFailed {cloud: String},

    #[error("Couldn't load certificate, provide valid certificate path in configuration. Use 'tedge config --set'")]
    Certificate,

    #[error(transparent)]
    Configuration(#[from] ConfigError),

    #[error("Connection is already established. To remove existing connection use 'tedge disconnect {cloud}' and try again.",)]
    ConfigurationExists {cloud: String},

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error("Required configuration item is not provided '{item}', run 'tedge config set {item} <value>' to add it to config.")]
    MissingRequiredConfigurationItem { item: String },

    #[error(transparent)]
    MqttClient(#[from] mqtt_client::Error),

    #[error(transparent)]
    PathsError(#[from] paths::PathsError),

    #[error(transparent)]
    PersistError(#[from] PersistError),

    #[error("Couldn't find path to 'sudo'. Update $PATH variable with 'sudo' path.\n{0}")]
    SudoNotFound(#[from] which::Error),

    #[error("Provided endpoint url is not valid, provide valid url.\n{0}")]
    UrlParse(#[from] url::ParseError),

    #[error(transparent)]
    ServicesError(#[from] services::ServicesError),
}

