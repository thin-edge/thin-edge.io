use crate::command::{BuildCommand, Command};
use crate::config::{ConfigError, TEdgeConfig};

use structopt::StructOpt;
/*
use tempfile::PersistError;
use crate::utils::{paths,services};
use crate::config::ConfigError;

mod c8y;
mod az;
*/
#[derive(StructOpt, Debug)]
pub enum TEdgeConnectOpt {
    /// Create connection to Cumulocity
    ///
    /// The command will create config and start edge relay from the device to c8y instance
    C8y,

    /// Create connection to Azure 
    ///
    /// The command will create config and start edge relay from the device to az instance
    AZ,
}

impl BuildCommand for TEdgeConnectOpt {
    fn build_command(
        self,
        config: crate::config::TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::config::ConfigError> {
      let cmd = match self {
            TEdgeConnectOpt::C8y => BridgeCommand {cloud_type:TEdgeConnectOpt::C8y},
            TEdgeConnectOpt::AZ => BridgeCommand{cloud_type:TEdgeConnectOpt::AZ},
        };
      Ok(cmd.into_boxed())
    }
}

pub struct BridgeCommand {
 cloud_type:TEdgeConnectOpt,
}

impl Command for BridgeCommand{
   fn description(&self) -> String {
        format!("Command to create bridge to connect to either to Cumulocity or Azure cloud")
    }

   fn execute(&self, _verbose: u8) -> Result<(), anyhow::Error> {
    
      //initialize the bridge config struct
      //Only check will differ
       match self.cloud_type {
           TEdgeConnectOpt::AZ => {
                 println!("Connect to azure cloud...................");
           }
           TEdgeConnectOpt::C8y =>{
               println!("Connect to c8y cloud....................");
           }
           _=> { println!("......Wrong Cloud or Not Supported....");}
       }
        Ok(())
    }
}


/*
#[AlreadyExistsrive(thiserror::Error, Debug)]
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
*/

/*
 //trait with check connection
//and config
/*
pub trait Brdige {
    check_connection(&self)->Result<(),anyhow::Error>
}
*/
    
#[derive(Debug, PartialEq)]
pub struct BridgeConfig {
        connection: String,
        address: String,
        remote_username: String,
        bridge_cafile: String,
        remote_clientid: String,
        bridge_certfile: String,
        bridge_keyfile: String,
        try_private: bool,
        start_type: String,
        cleansession: bool,
        bridge_insecure: bool,
        notifications: bool,
        bridge_attempt_unsubscribe: bool,
        topics: Vec<String>,
}

*
*/
