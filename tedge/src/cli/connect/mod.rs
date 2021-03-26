use crate::cli::connect::{az::Azure, c8y::C8y};
use crate::command::{BuildCommand, Command, ExecutionContext};
use crate::config::{ConfigError, TEdgeConfig};
use crate::services::{
    self, mosquitto::MosquittoService, tedge_mapper::TedgeMapperService, SystemdService,
};
use crate::utils::paths;
use crate::utils::users::UserManager;
use std::path::Path;
use structopt::StructOpt;
use tempfile::{NamedTempFile, PersistError};
use url::Url;
use which::which;

pub mod az;
pub mod c8y;

use crate::config::{
    AZURE_ROOT_CERT_PATH, AZURE_URL, C8Y_ROOT_CERT_PATH, C8Y_URL, DEVICE_CERT_PATH, DEVICE_ID,
    DEVICE_KEY_PATH,
};

pub const COMMON_MOSQUITTO_CONFIG_FILENAME: &str = "tedge-mosquitto.conf";
const DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";
const MOSQUITTO_RESTART_TIMEOUT_SECONDS: u64 = 5;
const MQTT_TLS_PORT: u16 = 8883;
pub const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "mosquitto-conf";
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
            "create bridge to connect {} cloud.",
            self.bridge_config.local_clientid
        )
    }

    fn execute(&self, context: &ExecutionContext) -> Result<(), anyhow::Error> {
        self.bridge_config.new_bridge(&context.user_manager)?;
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
struct CommonMosquittoConfig {
    config_file: String,
    connection_messages: bool,
    log_types: Vec<String>,
}

impl Default for CommonMosquittoConfig {
    fn default() -> Self {
        CommonMosquittoConfig {
            config_file: COMMON_MOSQUITTO_CONFIG_FILENAME.into(),
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
    common_mosquitto_config: CommonMosquittoConfig,
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
    use_mapper: bool,
    try_private: bool,
    start_type: String,
    clean_session: bool,
    notifications: bool,
    bridge_attempt_unsubscribe: bool,
    topics: Vec<String>,
}

trait CheckConnection {
    fn check_connection(&self) -> Result<(), ConnectError>;
}

impl BridgeConfig {
    fn new_bridge(&self, user_manager: &UserManager) -> Result<(), ConnectError> {
        println!("Checking if systemd is available.\n");
        let _ = services::systemd_available()?;

        println!("Checking if configuration for requested bridge already exists.\n");
        let _ = self.bridge_config_exists()?;

        println!("Validating the bridge certificates.\n");
        self.validate()?;

        println!("Saving configuration for requested bridge.\n");
        if let Err(err) = self.write_bridge_config_to_file() {
            // We want to preserve previous errors and therefore discard result of this function.
            let _ = self.clean_up();
            return Err(err);
        }

        println!("Restarting mosquitto service.\n");
        if let Err(err) = MosquittoService.restart(user_manager) {
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
        if let Err(err) = MosquittoService.enable(user_manager) {
            self.clean_up()?;
            return Err(err.into());
        }

        println!("Successfully created bridge connection!\n");

        if self.use_mapper {
            println!("Checking if tedge-mapper is installed.\n");

            if which("tedge_mapper").is_err() {
                println!("Warning: tedge_mapper is not installed. We recommend to install it.\n");
            } else {
                self.start_and_enable_tedge_mapper(user_manager);
            }
        }

        Ok(())
    }

    // To preserve error chain and not discard other errors we need to ignore error here
    // (don't use '?' with the call to this function to preserve original error).
    fn clean_up(&self) -> Result<(), ConnectError> {
        let path = self.get_bridge_config_file_path()?;
        let _ = std::fs::remove_file(&path).or_else(ok_if_not_found)?;
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
        let dir_path = paths::build_path_for_sudo_or_user(&[TEDGE_BRIDGE_CONF_DIR_PATH])?;

        // This will forcefully create directory structure if it doesn't exist, we should find better way to do it, maybe config should deal with it?
        let _ = paths::create_directories(&dir_path)?;

        let mut common_temp_file = NamedTempFile::new()?;
        self.serialize_common_config(&mut common_temp_file)?;
        let common_config_path = self.get_common_mosquitto_config_file_path()?;
        let _ = paths::persist_tempfile(common_temp_file, &common_config_path)?;

        let mut temp_file = NamedTempFile::new()?;
        self.serialize(&mut temp_file)?;
        let config_path = self.get_bridge_config_file_path()?;
        let _ = paths::persist_tempfile(temp_file, &config_path)?;

        Ok(())
    }

    fn serialize_common_config<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(
            writer,
            "connection_messages {}",
            self.common_mosquitto_config.connection_messages
        )?;

        for log_type in &self.common_mosquitto_config.log_types {
            writeln!(writer, "log_type {}", log_type)?;
        }

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
        writeln!(writer, "try_private {}", self.try_private)?;
        writeln!(writer, "start_type {}", self.start_type)?;
        writeln!(writer, "cleansession {}", self.clean_session)?;
        writeln!(writer, "notifications {}", self.notifications)?;
        writeln!(
            writer,
            "bridge_attempt_unsubscribe {}",
            self.bridge_attempt_unsubscribe
        )?;

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
        Ok(paths::build_path_for_sudo_or_user(&[
            TEDGE_BRIDGE_CONF_DIR_PATH,
            &self.config_file,
        ])?)
    }

    fn start_and_enable_tedge_mapper(&self, user_manager: &UserManager) {
        let mut failed = false;

        println!("Starting tedge-mapper service.\n");
        if let Err(err) = TedgeMapperService.restart(user_manager) {
            println!("Failed to stop tedge-mapper service: {:?}", err);
            failed = true;
        }

        println!("Persisting tedge-mapper on reboot.\n");
        if let Err(err) = TedgeMapperService.enable(user_manager) {
            println!("Failed to enable tedge-mapper service: {:?}", err);
            failed = true;
        }

        if !failed {
            println!("tedge-mapper service successfully started and enabled!\n");
        }
    }

    fn get_common_mosquitto_config_file_path(&self) -> Result<String, ConnectError> {
        Ok(paths::build_path_for_sudo_or_user(&[
            TEDGE_BRIDGE_CONF_DIR_PATH,
            &self.common_mosquitto_config.config_file,
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

    #[error("Provided endpoint url is not valid, provide valid url.\n{0}")]
    UrlParse(#[from] url::ParseError),

    #[error(transparent)]
    ServicesError(#[from] services::ServicesError),
}

fn ok_if_not_found(err: std::io::Error) -> std::io::Result<()> {
    match err.kind() {
        std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(err),
    }
}

#[cfg(test)]
mod test;
