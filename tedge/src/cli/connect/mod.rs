use crate::cli::connect::{az::Azure, c8y::C8y};
use crate::command::{BuildCommand, Command};
use crate::config::{ConfigError, TEdgeConfig};

use crate::utils::{paths, services};
use std::path::Path;
use structopt::StructOpt;
use tempfile::{NamedTempFile, PersistError};
use url::Url;

pub mod az;
pub mod c8y;

use crate::config::{
    AZURE_ROOT_CERT_PATH, AZURE_URL, C8Y_ROOT_CERT_PATH, C8Y_URL, DEVICE_CERT_PATH, DEVICE_ID,
    DEVICE_KEY_PATH, TEDGE_HOME_DIR,
};

const DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";
const MOSQUITTO_RESTART_TIMEOUT_SECONDS: u64 = 5;
const MQTT_TLS_PORT: u16 = 8883;
pub const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "bridges";
const WAIT_FOR_CHECK_SECONDS: u64 = 10;
const PATH_TO_TEDGE_MAPPER_BIN: &str = "/usr/bin/tedge_mapper";

#[derive(StructOpt, Debug, PartialEq)]
pub enum TEdgeConnectOpt {
    /// Create connection to Cumulocity
    ///
    /// The command will create config and start edge relay from the device to c8y instance
    C8y,

    /// Create connection to Azure
    ///
    /// The command will create config and start edge relay from the device to az instance
    Az,
}

impl BuildCommand for TEdgeConnectOpt {
    fn build_command(
        self,
        tedge_config: crate::config::TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::config::ConfigError> {
        let cmd = match self {
            TEdgeConnectOpt::C8y => BridgeCommand {
                bridge_config: C8y::c8y_bridge_config(tedge_config)?,
                check_connection: Box::new(C8y {}),
            },
            TEdgeConnectOpt::Az => BridgeCommand {
                bridge_config: Azure::azure_bridge_config(tedge_config)?,
                check_connection: Box::new(Azure {}),
            },
        };
        Ok(cmd.into_boxed())
    }
}

pub struct BridgeCommand {
    bridge_config: BridgeConfig,
    check_connection: Box<dyn CheckConnection>,
}

impl Command for BridgeCommand {
    fn description(&self) -> String {
        format!(
            "Create bridge to connect {} cloud",
            self.bridge_config.local_clientid
        )
    }

    fn execute(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        self.bridge_config.new_bridge()?;
        self.check_connection()?;
        Ok(())
    }
}

impl BridgeCommand {
    fn check_connection(&self) -> Result<(), ConnectError> {
        println!(
            "Sending packets to check connection. This may take up to {} seconds.\n",
            WAIT_FOR_CHECK_SECONDS
        );
        Ok(self.check_connection.check_connection()?)
    }
}

#[derive(Debug, PartialEq)]
struct CommonBridgeConfig {
    try_private: bool,
    start_type: String,
    clean_session: bool,
    notifications: bool,
    bridge_attempt_unsubscribe: bool,
    bind_address: String,
    connection_messages: bool,
    log_types: Vec<String>,
}

impl Default for CommonBridgeConfig {
    fn default() -> Self {
        CommonBridgeConfig {
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            notifications: false,
            bridge_attempt_unsubscribe: false,
            bind_address: "127.0.0.1".into(),
            connection_messages: true,
            log_types: vec![
                "error".into(),
                "warning".into(),
                "notice".into(),
                "information".into(),
                "subscribe".into(),
                "unsubscribe".into(),
            ],
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct BridgeConfig {
    common_bridge_config: CommonBridgeConfig,
    cloud_name: String,
    config_file: String,
    connection: String,
    address: String,
    remote_username: Option<String>,
    bridge_root_cert_path: String,
    remote_clientid: String,
    local_clientid: String,
    bridge_certfile: String,
    bridge_keyfile: String,
    mapper: bool,
    topics: Vec<String>,
}

trait CheckConnection {
    fn check_connection(&self) -> Result<(), ConnectError>;
}

impl BridgeConfig {
    fn new_bridge(&self) -> Result<(), ConnectError> {
        println!("Checking if systemd and mosquitto are available.\n");
        let _ = services::all_services_available()?;

        println!("Checking if configuration for requested bridge already exists.\n");
        let _ = self.bridge_config_exists()?;

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

        println!("Persisting mosquitto on reboot.\n");
        if let Err(err) = services::mosquitto_enable_daemon() {
            self.clean_up()?;
            return Err(err.into());
        }

        println!("Successfully created bridge connection!\n");

        if self.mapper {
            println!("Checking if tedge-mapper is installed.\n");

            if !(Path::new(PATH_TO_TEDGE_MAPPER_BIN).exists()) {
                println!("Warning: tedge_mapper is not installed. We recommend to install it.\n");
            } else {
                println!("Starting tedge-mapper service.\n");
                let _ = services::tedge_mapper_start_daemon();

                println!("Persisting tedge-mapper on reboot.\n");
                let _ = services::tedge_mapper_enable_daemon();

                println!("tedge-mapper service successfully started and enabled!\n");
            }
        }

        Ok(())
    }

    // To preserve error chain and not discard other errors we need to ignore error here
    // (don't use '?' with the call to this function to preserve original error).
    fn clean_up(&self) -> Result<(), ConnectError> {
        let path = self.get_bridge_config_file_path()?;
        let _ = std::fs::remove_file(&path).or_else(services::ok_if_not_found)?;
        Ok(())
    }

    fn bridge_config_exists(&self) -> Result<(), ConnectError> {
        let path = self.get_bridge_config_file_path()?;
        if Path::new(&path).exists() {
            return Err(ConnectError::ConfigurationExists {
                cloud: self.cloud_name.to_string(),
            });
        }
        Ok(())
    }

    fn write_bridge_config_to_file(&self) -> Result<(), ConnectError> {
        let mut temp_file = NamedTempFile::new()?;
        self.serialize(&mut temp_file)?;

        let dir_path = paths::build_path_from_home(&[TEDGE_HOME_DIR, TEDGE_BRIDGE_CONF_DIR_PATH])?;

        // This will forcefully create directory structure if it doesn't exist, we should find better way to do it, maybe config should deal with it?
        let _ = paths::create_directories(&dir_path)?;

        let config_path = self.get_bridge_config_file_path()?;
        let _ = paths::persist_tempfile(temp_file, &config_path)?;

        Ok(())
    }

    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(writer, "### Bridge",)?;
        writeln!(writer, "connection {}", self.connection)?;
        //write azure specific configuration to file
        match &self.remote_username {
            Some(name) => {
                writeln!(writer, "remote_username {}", name)?;
            }
            None => {}
        }
        writeln!(writer, "address {}", self.address)?;

        if std::fs::metadata(&self.bridge_root_cert_path)?.is_dir() {
            writeln!(writer, "bridge_capath {}", self.bridge_root_cert_path)?;
        } else {
            writeln!(writer, "bridge_cafile {}", self.bridge_root_cert_path)?;
        }

        writeln!(writer, "remote_clientid {}", self.remote_clientid)?;
        writeln!(writer, "local_clientid {}", self.local_clientid)?;
        writeln!(writer, "bridge_certfile {}", self.bridge_certfile)?;
        writeln!(writer, "bridge_keyfile {}", self.bridge_keyfile)?;
        writeln!(
            writer,
            "try_private {}",
            self.common_bridge_config.try_private
        )?;
        writeln!(
            writer,
            "start_type {}",
            self.common_bridge_config.start_type
        )?;
        writeln!(
            writer,
            "cleansession {}",
            self.common_bridge_config.clean_session
        )?;
        writeln!(
            writer,
            "notifications {}",
            self.common_bridge_config.notifications
        )?;
        writeln!(
            writer,
            "bridge_attempt_unsubscribe {}",
            self.common_bridge_config.bridge_attempt_unsubscribe
        )?;
        writeln!(
            writer,
            "bind_address {}",
            self.common_bridge_config.bind_address
        )?;
        writeln!(
            writer,
            "connection_messages {}",
            self.common_bridge_config.connection_messages
        )?;

        for log_type in &self.common_bridge_config.log_types {
            writeln!(writer, "log_type {}", log_type)?;
        }

        writeln!(writer, "\n### Topics",)?;
        for topic in &self.topics {
            writeln!(writer, "topic {}", topic)?;
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), ConnectError> {
        Url::parse(&self.address)?;

        if !Path::new(&self.bridge_root_cert_path).exists() {
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

    fn get_bridge_config_file_path(&self) -> Result<String, ConnectError> {
        Ok(paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            &self.config_file,
        ])?)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ConnectError {
    #[error("Couldn't load certificate, provide valid certificate path in configuration. Use 'tedge config --set'")]
    Certificate,

    #[error(transparent)]
    Configuration(#[from] ConfigError),

    #[error("Connection is already established. To remove existing connection use 'tedge disconnect {cloud}' and try again.",)]
    ConfigurationExists { cloud: String },

    #[error(transparent)]
    IoError(#[from] std::io::Error),

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

#[cfg(test)]
mod test;
