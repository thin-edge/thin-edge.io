use crate::command::{BuildCommand, Command};
use crate::config::{ConfigError, TEdgeConfig};
use crate::cli::connect::az::Azure;
use std::path::Path;
use tempfile::NamedTempFile;
use url::Url;

use structopt::StructOpt;

use tempfile::PersistError;
use crate::utils::{paths,services};

mod az;

use crate::config::{
    CLOUD_CONNECT, CLOUD_ROOT_CERT_PATH, CLOUD_URL, DEVICE_CERT_PATH,
    DEVICE_ID, DEVICE_KEY_PATH, TEDGE_HOME_DIR,
};

const AZURE_CONFIG_FILENAME: &str = "az-bridge.conf";
const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";
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
            TEdgeConnectOpt::C8y => BridgeCommand {cloud_type:TEdgeConnectOpt::C8y},
            TEdgeConnectOpt::AZ => BridgeCommand {cloud_type:TEdgeConnectOpt::AZ},
        };
      Ok(cmd.into_boxed())
    }
}

pub struct BridgeCommand {
 cloud_type:TEdgeConnectOpt,
}

impl Command for BridgeCommand{
   fn description(&self) -> String {
        format!("Create bridge to connect Cumulocity/Azure cloud")
    }

   fn execute(&self, _verbose: u8) -> Result<(), anyhow::Error> {
      //initialize the bridge config struct
      //Only check will differ
       match self.cloud_type {
           TEdgeConnectOpt::AZ => {
                 println!("Connect to azure cloud...................");
                 let mut bridge = BridgeConfig::default();
                 bridge.cloud_type = TEdgeConnectOpt::AZ;
                 bridge.new_bridge()?
           }
           TEdgeConnectOpt::C8y =>{
                 println!("Connect to c8y cloud....................");
                 let mut bridge = BridgeConfig::default();
                 bridge.cloud_type = TEdgeConnectOpt::C8y;
                 bridge.new_bridge()?
           }
       };
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub struct BridgeConfig {
        cloud_type: TEdgeConnectOpt,
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

impl BridgeConfig {
    fn new_bridge(&mut self) -> Result<(), ConnectError> {
        println!("Checking if systemd and mosquitto are available.\n");
        let _ = services::all_services_available()?;

        println!("Checking if configuration for requested bridge already exists.\n");
        let _ = self.config_exists()?;

        println!("Creating configuration for requested bridge.\n");
        self.load_config()?;

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

        match  self.cloud_type {
            TEdgeConnectOpt::AZ => { 
                 println!("Check azure cloud Connection...................");
                //az.check_connection()?;
                }
            TEdgeConnectOpt::C8y => { 
                 println!("Check c8y cloud Connection...................");
                //c8y.check_connection()?;
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
           let config_file_path: String;
           match self.cloud_type {
                TEdgeConnectOpt::AZ => {
                    config_file_path = AZURE_CONFIG_FILENAME.to_string();
                }
                TEdgeConnectOpt::C8y => {
                    config_file_path = C8Y_CONFIG_FILENAME.to_string();
                }
            }
        let path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            &config_file_path,
        ])?;
        let _ = std::fs::remove_file(&path).or_else(services::ok_if_not_found)?;

        Ok(())
    }

        

    fn config_exists(&self) -> Result<(), ConnectError> {
        let path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            AZURE_CONFIG_FILENAME,
        ])?;

        if Path::new(&path).exists() {
            match self.cloud_type {
                TEdgeConnectOpt::AZ => {
                    return Err(ConnectError::ConfigurationExists {cloud: String::from("az")});
                }
                TEdgeConnectOpt::C8y => {
                    return Err(ConnectError::ConfigurationExists {cloud: String::from("c8y")});
                }
            }
        }

        Ok(())
    }

    fn load_config(&mut self) -> Result<(), ConnectError> {
       self.try_new()?;
       Ok(self.validate()?)
    }

    fn save_bridge_config(&self) -> Result<(), ConnectError> {
        let mut config = TEdgeConfig::from_default_config()?;
        TEdgeConfig::set_config_value(&mut config, CLOUD_CONNECT, "true".into())?;
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
            AZURE_CONFIG_FILENAME,
        ])?;

        let _ = paths::persist_tempfile(temp_file, &config_path)?;

        Ok(())
    }

   fn try_new(&mut self) -> Result<(), ConnectError> {
        let config = TEdgeConfig::from_default_config()?;
        self.address = get_config_value(&config, CLOUD_URL)?;
        self.remote_clientid = get_config_value(&config, DEVICE_ID)?;
        let iothub_name: Vec<&str> = self.address.split(":").collect();  
        match self.cloud_type  {
            TEdgeConnectOpt::AZ => {
                   self.remote_username = format!("{}",iothub_name.into_iter().nth(0).unwrap())+"/"+&self.remote_clientid.to_string()+"/?api-version=2018-06-30";
            }
            _=> {}
        }
        self.bridge_cafile = get_config_value(&config, CLOUD_ROOT_CERT_PATH)?;
        self.bridge_certfile = get_config_value(&config, DEVICE_CERT_PATH)?;
        self.bridge_keyfile = get_config_value(&config, DEVICE_KEY_PATH)?;
        Ok(())
    }

    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(writer, "### Bridge",)?;
        writeln!(writer, "connection {}", self.connection)?;
        writeln!(writer, "address {}", self.address)?;
        writeln!(writer, "bridge_cafile {}", self.bridge_cafile)?;
        writeln!(writer, "remote_clientid {}", self.remote_clientid)?;
        writeln!(writer, "remote_username {}", self.remote_username)?;
        writeln!(writer, "bridge_certfile {}", self.bridge_certfile)?;
        writeln!(writer, "bridge_keyfile {}", self.bridge_keyfile)?;
        writeln!(writer, "try_private {}", self.try_private)?;
        writeln!(writer, "start_type {}", self.start_type)?;
        writeln!(writer, "cleansession {}", self.cleansession)?;
        writeln!(writer, "bridge_insecure {}", self.bridge_insecure)?;
        writeln!(writer, "notifications {}", self.notifications)?;
        writeln!(writer, "bridge_attempt_unsubscribe {}", self.bridge_attempt_unsubscribe)?;
        match self.cloud_type {
           TEdgeConnectOpt::AZ => { 
                let az_topics = Azure::get_azure_topics(self); 
                writeln!(writer, "\n### Topics",)?;
                for topic in &az_topics {
                    writeln!(writer, "topic {}", topic)?;
                }
           }
           TEdgeConnectOpt::C8y => {
               //Initialize C8y topics
           }
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
/// Mosquitto config parameters required for bridge to be established:
/// cloud_type azure/c8y
/// # CLOUD Bridge
/// connection edge_to_az
/// address mqtt.$CLOUD_URL:8883
/// bridge_cafile $CLOUD_CERT
/// remote_clientid $DEVICE_ID
/// remote_username $CLOUD_USERNAME
/// bridge_certfile $CERT_PATH
/// bridge_keyfile $KEY_PATH
/// try_private false
/// start_type automatic
impl Default for BridgeConfig {
    fn default() -> BridgeConfig {
        BridgeConfig {
            cloud_type:TEdgeConnectOpt::C8y,
            connection: "edge_to_az".into(),
            address: "".into(),
            remote_username: "".into(),
            bridge_cafile: "".into(),
            remote_clientid: "alpha".into(),
            bridge_certfile: "".into(),
            bridge_keyfile: "".into(),
            try_private: false,
            start_type: "automatic".into(),
            cleansession: true,
            bridge_insecure: false,
            notifications: false,
            bridge_attempt_unsubscribe: false,
            topics: vec![
                //Cloud specific topics to be added later 
            ],
        }
    }
}

fn get_config_value(config: &TEdgeConfig, key: &str) -> Result<String, ConnectError> {
    Ok(config
        .get_config_value(key)?
        .ok_or_else(|| ConnectError::MissingRequiredConfigurationItem { item: key.into() })?)
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
