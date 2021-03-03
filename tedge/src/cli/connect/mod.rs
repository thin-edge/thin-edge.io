use crate::cli::connect::az::Azure;
use crate::cli::connect::c8y::C8y;
use crate::command::{BuildCommand, Command};
use crate::config::{ConfigError, TEdgeConfig};

use crate::utils::{paths, services};
use std::path::Path;
use structopt::StructOpt;
use tempfile::NamedTempFile;
use tempfile::PersistError;
use url::Url;

mod az;
mod c8y;

use crate::config::{
    C8Y_CONNECT, C8Y_ROOT_CERT_PATH, C8Y_URL, DEVICE_CERT_PATH, DEVICE_ID, DEVICE_KEY_PATH,
    TEDGE_HOME_DIR, _AZURE_CONNECT, _AZURE_ROOT_CERT_PATH, _AZURE_URL,
};

const MOSQUITTO_RESTART_TIMEOUT_SECONDS: u64 = 5;
const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "bridges";
const WAIT_FOR_CHECK_SECONDS: u64 = 10;

#[derive(StructOpt, Debug, PartialEq)]
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
        _config: crate::config::TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::config::ConfigError> {
        let cmd = match self {
            TEdgeConnectOpt::C8y => BridgeCommand {
                bridge_config: C8y::c8y_bridge_config()?,
            },
            TEdgeConnectOpt::AZ => BridgeCommand {
                bridge_config: Azure::azure_bridge_config()?,
            },
        };
        Ok(cmd.into_boxed())
    }
}

pub struct BridgeCommand {
    bridge_config:  BridgeConfig,
}

impl Command for BridgeCommand {
    fn description(&self) -> String {
        format!("Create bridge to connect Cumulocity/Azure cloud")
    }

    fn execute(&self, _verbose: u8) -> Result<(), anyhow::Error> {
             self.bridge_config.new_bridge()?;
             Ok(())
        }
}

#[derive(Debug, PartialEq)]
pub struct BridgeConfig {
    cloud_type: TEdgeConnectOpt,
    cloud_name: String,
    config_file: String,
    connection: String,
    address: String,
    remote_username: String,
    bridge_cafile: String,
    remote_clientid: String,
    local_clientid: String,
    bridge_certfile: String,
    bridge_keyfile: String,
    try_private: bool,
    start_type: String,
    cleansession: bool,
    bridge_insecure: bool,
    notifications: bool,
    bridge_attempt_unsubscribe: bool,
    topics: Vec<String>,
    cloud_connect: String,
}

impl BridgeConfig {
    #[tokio::main]
    async fn new_bridge(&self) -> Result<(), ConnectError> {
        println!("Checking if systemd and mosquitto are available.\n");
        let _ = services::all_services_available()?;

        println!("Checking if configuration for requested bridge already exists.\n");
        let _ = self.config_exists()?;

        println!("Validate the bridge certificates.\n");
        self.validate()?;

        println!("Saving configuration for requested bridge.\n");
        if let Err(err) = self.write_bridge_config_to_file() {
            // We want to preserve previous errors and therefore discard result of this function.
            let _ = self.clean_up();
            return Err(err);
        }
        println!("Restarting mosquitto, [requires elevated permission], authorise when asked.\n");
        if let Err(err) = services::mosquitto_restart_daemon() {
            self.clean_up()?;
            return Err(err.into());
        }
        println!(
            "Awaiting mosquitto to start. This may take up to {} seconds.\n",
            MOSQUITTO_RESTART_TIMEOUT_SECONDS
        );
        std::thread::sleep(std::time::Duration::from_secs(
            MOSQUITTO_RESTART_TIMEOUT_SECONDS,
        ));

        println!(
            "Sending packets to check connection. This may take up to {} seconds.\n",
            WAIT_FOR_CHECK_SECONDS
        );

        match self.cloud_type {
            TEdgeConnectOpt::AZ => {
                //Azure::check_connection()?;
            }
            TEdgeConnectOpt::C8y => {
                C8y::check_connection().await?;
            }
        }

        println!("Persisting mosquitto on reboot.\n");
        if let Err(err) = services::mosquitto_enable_daemon() {
            self.clean_up()?;
            return Err(err.into());
        }

        println!("Saving configuration.");
        self.save_bridge_config()?;

        println!("Successfully created bridge connection!");

        Ok(())
    }

    // To preserve error chain and not discard other errors we need to ignore error here
    // (don't use '?' with the call to this function to preserve original error).
    fn clean_up(&self) -> Result<(), ConnectError> {
        let path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            &self.config_file,
        ])?;
        let _ = std::fs::remove_file(&path).or_else(services::ok_if_not_found)?;

        Ok(())
    }

    fn config_exists(&self) -> Result<(), ConnectError> {
        let path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            &self.config_file,
        ])?;

        if Path::new(&path).exists() {
              return Err(ConnectError::ConfigurationExists {
                        cloud: self.cloud_name.to_string(),
              });

         }
         Ok(())
    }

    fn save_bridge_config(&self) -> Result<(), ConnectError> {
        let mut config = TEdgeConfig::from_default_config()?;
        TEdgeConfig::set_config_value(&mut config, &self.cloud_connect, "true".into())?;
        Ok(TEdgeConfig::write_to_default_config(&config)?)
    }

    fn write_bridge_config_to_file(&self) -> Result<(), ConnectError> {
        let mut temp_file = NamedTempFile::new()?;
        self.serialize(&mut temp_file)?;

        let dir_path = paths::build_path_from_home(&[TEDGE_HOME_DIR, TEDGE_BRIDGE_CONF_DIR_PATH])?;

        // This will forcefully create directory structure if it doesn't exist, we should find better way to do it, maybe config should deal with it?
        let _ = paths::create_directories(&dir_path)?;

        let config_path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            &self.config_file,
        ])?;

        let _ = paths::persist_tempfile(temp_file, &config_path)?;

        Ok(())
    }

    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(writer, "### Bridge",)?;
        writeln!(writer, "connection {}", self.connection)?;
        //write azure specific configuration to file
        match self.cloud_name.as_str() {
               "az" => {
                writeln!(writer, "remote_username {}", self.remote_username)?;
                writeln!(writer, "cleansession {}", self.cleansession)?;
                writeln!(writer, "bridge_insecure {}", self.bridge_insecure)?;
                writeln!(writer, "notifications {}", self.notifications)?;
                writeln!(
                    writer,
                    "bridge_attempt_unsubscribe {}",
                    self.bridge_attempt_unsubscribe
                )?;
            }
            _ => {}
        }

        writeln!(writer, "address {}", self.address)?;
        writeln!(writer, "bridge_cafile {}", self.bridge_cafile)?;
        writeln!(writer, "remote_clientid {}", self.remote_clientid)?;
        writeln!(writer, "local_clientid {}", self.local_clientid)?;
        writeln!(writer, "bridge_certfile {}", self.bridge_certfile)?;
        writeln!(writer, "bridge_keyfile {}", self.bridge_keyfile)?;
        writeln!(writer, "try_private {}", self.try_private)?;
        writeln!(writer, "start_type {}", self.start_type)?;
        writeln!(writer, "\n### Topics",)?;
        for topic in &self.topics {
            writeln!(writer, "topic {}", topic)?;
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), ConnectError> {
        Url::parse(&self.address)?;

        if !Path::new(&self.bridge_cafile).exists() {
            return Err(ConnectError::Certificate);
        }

        if !Path::new(&self.bridge_certfile).exists() {
            return Err(ConnectError::Certificate);
        }

        if !Path::new(&self.bridge_keyfile).exists() {
            return Err(ConnectError::Certificate);
        }

        Ok(())
    }
}

fn get_config_value(config: &TEdgeConfig, key: &str) -> Result<String, ConfigError> {
    config
        .get_config_value(key)?
        .ok_or_else(||ConfigError::ConfigNotSet{key: key.into()})
}

#[derive(thiserror::Error, Debug)]
pub enum ConnectError {
    #[error("Bridge has been configured, but {cloud} connection check failed.")]
    BridgeConnectionFailed { cloud: String },

    #[error("Couldn't load certificate, provide valid certificate path in configuration. Use 'tedge config --set'")]
    Certificate,

    #[error(transparent)]
    Configuration(#[from] ConfigError),

    #[error("Connection is already established. To remove existing connection use 'tedge disconnect {cloud}' and try again.",)]
    ConfigurationExists { cloud: String },

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
