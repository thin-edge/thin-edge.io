use std::path::Path;
use std::time::Duration;

use structopt::StructOpt;
use tempfile::{NamedTempFile, PersistError};
use tokio::time::timeout;
use url::Url;

use super::utils;
use crate::command::Command;
use crate::config::{
    ConfigError, TEdgeConfig, C8Y_ROOT_CERT_PATH, C8Y_URL, DEVICE_CERT_PATH, DEVICE_ID,
    DEVICE_KEY_PATH,
};
use mqtt_client::{Client, Message, Topic};

const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";
const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "bridges";
const TEDGE_HOME_PREFIX: &str = ".tedge";

#[derive(thiserror::Error, Debug)]
pub enum ConnectError {
    #[error("Bridge connection has not been established, check configuration and try again.")]
    BridgeConnectionFailed,

    #[error("Couldn't load certificate, please provide valid certificate path in configuration.")]
    Certificate,

    #[error("")]
    Configuration(#[from] ConfigError),

    #[error("Connection cannot be established as config already exists. Please remove existing configuration for the bridge and try again.")]
    ConfigurationExists,

    #[error("Required configuration item is not provided [{item}], run 'tedge config set {item}' to add it to your config.")]
    MissingRequiredConfigurationItem { item: String },

    #[error("Couldn't set MQTT Server to start on boot.")]
    MosquittoCantPersist,

    #[error("MQTT Server is not available on the system, it is required to use this command.")]
    MosquittoNotAvailable,

    #[error("MQTT Server is not available on the system as a service, it is required to use this command.")]
    MosquittoNotAvailableAsService,

    #[error("MQTT Server is active on the system as a service, please stop the service before you use this command.")]
    MosquittoIsActive,

    #[error("MQTT client failed.")]
    MqttClient(#[from] mqtt_client::Error),

    #[error("Couldn't write configutation file, ")]
    PersistError(#[from] PersistError),

    #[error("IO Error.")]
    StdIoError(#[from] std::io::Error),

    #[error("Couldn't find path to 'sudo'.")]
    SudoNotFound(#[from] which::Error),

    #[error("Systemd is not available on the system or elevated permissions have not been granted, it is required to use this command.")]
    SystemdNotAvailable,

    #[error("Returned error is not recognised: {code:?}.")]
    UnknownReturnCode { code: Option<i32> },

    #[error("Provided endpoint url is not valid, please provide valid url.")]
    UrlParse(#[from] url::ParseError),
}

#[derive(StructOpt, Debug)]
pub struct Connect {}

impl Command for Connect {
    fn to_string(&self) -> String {
        "execute `tedge connect`.".into()
    }

    fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        Ok(self.new_bridge()?)
    }
}

impl Connect {
    fn new_bridge(&self) -> Result<(), ConnectError> {
        println!("Checking if systemd and mosquitto are available.\n");
        let _ = utils::all_services_available()?;

        println!("Checking if configuration for requested bridge already exists.\n");
        let _ = self.config_exists()?;

        println!("Creating configuration for requested bridge.\n");
        let config = self.load_config()?;

        println!("Saving configuration for requested bridge.\n");
        match self.write_bridge_config_to_file(&config) {
            Err(err) => {
                self.clean_up()?;
                return Err(err);
            }
            _ => {}
        }

        println!(
            "Restarting MQTT Server, [requires elevated permission], please authorise if asked.\n"
        );
        match utils::mosquitto_restart_daemon() {
            Err(err) => {
                self.clean_up()?;
                return Err(err);
            }
            _ => {}
        }

        const RESTART_TIMEOUT_SECONDS: u64 = 5;

        println!("Awaiting MQTT Server to start. This may take few seconds.\n");
        std::thread::sleep(std::time::Duration::from_secs(RESTART_TIMEOUT_SECONDS));

        println!("Sending packets to check connection.");
        match self.check_connection() {
            Err(err) => {
                self.clean_up()?;
                return Err(err);
            }
            _ => {}
        }

        println!("Persisting MQTT Server on reboot.\n");
        match utils::mosquitto_enable_daemon() {
            Err(err) => {
                self.clean_up()?;
                return Err(err);
            }
            _ => {}
        }

        println!("Successully created bridge connection!");
        Ok(())
    }

    fn clean_up(&self) -> Result<(), ConnectError> {
        fn ok_if_not_found(err: std::io::Error) -> std::io::Result<()> {
            match err.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(err),
            }
        }

        let path = utils::build_path_from_home(&[
            TEDGE_HOME_PREFIX,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            C8Y_CONFIG_FILENAME,
        ])?;
        let _ = std::fs::remove_file(&path).or_else(ok_if_not_found)?;

        Ok(())
    }

    // We are going to use c8y templates over mqtt to check if connectiom has been open.
    // Empty payload publish to s/ut/existingTemplateCollection
    // 20,existingTemplateCollection,<ID of collection>
    //
    // Empty payload publish to s/ut/notExistingTemplateCollection
    // 41,notExistingTemplateCollection
    // It seems to be appropriate to use the negative (second option) to check if template exists.
    #[tokio::main]
    async fn check_connection(&self) -> Result<(), ConnectError> {
        const WAIT_FOR_SECONDS: u64 = 5;

        const C8Y_TOPIC_TEMPLATE_DOWNSTREAM: &str = "c8y/s/dt";
        const C8Y_TOPIC_TEMPLATE_UPSTREAM: &str = "c8y/s/ut/notExistingTemplateCollection";
        const CLIENT_ID: &str = "check_connection";

        let template_pub_topic = Topic::new(C8Y_TOPIC_TEMPLATE_UPSTREAM)?;
        let template_sub_topic = Topic::new(C8Y_TOPIC_TEMPLATE_DOWNSTREAM)?;

        let mqtt = Client::connect(CLIENT_ID, &mqtt_client::Config::default()).await?;
        let mut template_response = mqtt.subscribe(template_sub_topic.filter()).await?;

        let (sender, receiver) = tokio::sync::oneshot::channel();

        let _task_handle = tokio::spawn(async move {
            while let Some(message) = template_response.next().await {
                if std::str::from_utf8(&message.payload)
                    .unwrap_or("")
                    .contains("41,notExistingTemplateCollection")
                {
                    let _ = sender.send(true);
                    break;
                }
            }
        });

        mqtt.publish(Message::new(&template_pub_topic, "")).await?;

        let fut = timeout(Duration::from_secs(WAIT_FOR_SECONDS), receiver);
        match fut.await {
            Ok(Ok(true)) => {
                println!("Received message.");
            }
            _err => {
                return Err(ConnectError::BridgeConnectionFailed);
            }
        }

        Ok(())
    }

    fn config_exists(&self) -> Result<(), ConnectError> {
        let path = utils::build_path_from_home(&[
            TEDGE_HOME_PREFIX,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            C8Y_CONFIG_FILENAME,
        ])?;

        if Path::new(&path).exists() {
            return Err(ConnectError::ConfigurationExists);
        }

        Ok(())
    }

    fn load_config(&self) -> Result<Config, ConnectError> {
        Config::new_c8y()?.validate()
    }

    fn write_bridge_config_to_file(&self, config: &Config) -> Result<(), ConnectError> {
        let mut temp_file = NamedTempFile::new()?;
        match config {
            Config::C8y(c8y) => c8y.serialize(&mut temp_file)?,
        }

        let dir_path =
            utils::build_path_from_home(&[TEDGE_HOME_PREFIX, TEDGE_BRIDGE_CONF_DIR_PATH])?;

        // This will forcefully create directory structure if doessn't exist, we should find better way to do it, maybe config should deal with it?
        let _ = std::fs::create_dir_all(dir_path)?;

        let config_path = utils::build_path_from_home(&[
            TEDGE_HOME_PREFIX,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            C8Y_CONFIG_FILENAME,
        ])?;

        temp_file.persist(config_path)?;

        Ok(())
    }
}

#[derive(Debug, PartialEq)]
enum Config {
    C8y(C8yConfig),
}

impl Config {
    fn new_c8y() -> Result<Config, ConnectError> {
        Ok(Config::C8y(C8yConfig::new()?))
    }

    fn validate(self) -> Result<Config, ConnectError> {
        match self {
            Config::C8y(config) => {
                config.validate()?;
                Ok(Config::C8y(config))
            }
        }
    }
}

#[derive(Debug, PartialEq)]
struct C8yConfig {
    connection: String,
    address: String,
    bridge_cafile: String,
    remote_clientid: String,
    bridge_certfile: String,
    bridge_keyfile: String,
    try_private: bool,
    start_type: String,
    topics: Vec<String>,
}

/// Mosquitto config parameters required for C8Y bridge to be estabilished:
/// # C8Y Bridge
/// connection edge_to_c8y
/// address mqtt.$C8Y_URL:8883
/// bridge_cafile $C8Y_CERT
/// remote_clientid $DEVICE_ID
/// bridge_certfile $CERT_PATH
/// bridge_keyfile $KEY_PATH
/// try_private false
/// start_type automatic
impl Default for C8yConfig {
    fn default() -> C8yConfig {
        C8yConfig {
            connection: "edge_to_c8y".into(),
            address: "".into(),
            bridge_cafile: "".into(),
            remote_clientid: "alpha".into(),
            bridge_certfile: "".into(),
            bridge_keyfile: "".into(),
            try_private: false,
            start_type: "automatic".into(),
            topics: vec![
                // Registration
                r#"s/dcr in 2 c8y/ """#.into(),
                r#"s/ucr out 2 c8y/ """#.into(),
                // Templates
                r#"s/dt in 2 c8y/ """#.into(),
                r#"s/ut/# out 2 c8y/ """#.into(),
                // Static templates
                r#"s/us out 2 c8y/ """#.into(),
                r#"t/us out 2 c8y/ """#.into(),
                r#"q/us out 2 c8y/ """#.into(),
                r#"c/us out 2 c8y/ """#.into(),
                r#"s/ds in 2 c8y/ """#.into(),
                r#"s/os in 2 c8y/ """#.into(),
                // Debug
                r#"s/e in 0 c8y/ """#.into(),
                // SmartRest2
                r#"s/uc/# out 2 c8y/ """#.into(),
                r#"t/uc/# out 2 c8y/ """#.into(),
                r#"q/uc/# out 2 c8y/ """#.into(),
                r#"c/uc/# out 2 c8y/ """#.into(),
                r#"s/dc/# in 2 c8y/ """#.into(),
                r#"s/oc/# in 2 c8y/ """#.into(),
                // c8y JSON
                r#"measurement/measurements/create out 2 c8y/ """#.into(),
                r#"error in 2 c8y/ """#.into(),
            ],
        }
    }
}

impl C8yConfig {
    fn new() -> Result<C8yConfig, ConnectError> {
        let bridge_config = C8yConfig::from_tedge_config()?;

        Ok(bridge_config)
    }

    fn from_tedge_config() -> Result<C8yConfig, ConnectError> {
        let config = TEdgeConfig::from_default_config()?;
        let address = get_config_value(&config, C8Y_URL)?;

        let remote_clientid = get_config_value(&config, DEVICE_ID)?;

        let bridge_cafile = get_config_value(&config, C8Y_ROOT_CERT_PATH)?;
        let bridge_certfile = get_config_value(&config, DEVICE_CERT_PATH)?;
        let bridge_keyfile = get_config_value(&config, DEVICE_KEY_PATH)?;

        Ok(C8yConfig {
            connection: "edge_to_c8y".into(),
            address,
            bridge_cafile,
            remote_clientid,
            bridge_certfile,
            bridge_keyfile,
            ..C8yConfig::default()
        })
    }

    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(writer, "### Bridge",)?;
        writeln!(writer, "connection {}", self.connection)?;
        writeln!(writer, "address {}", self.address)?;
        writeln!(writer, "bridge_cafile {}", self.bridge_cafile)?;
        writeln!(writer, "remote_clientid {}", self.remote_clientid)?;
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

fn get_config_value(config: &TEdgeConfig, key: &str) -> Result<String, ConnectError> {
    Ok(config
        .get_config_value(key)?
        .ok_or_else(|| ConnectError::MissingRequiredConfigurationItem { item: key.into() })?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORRECT_URL: &str = "http://test.com";
    const INCORRECT_URL: &str = "noturl";
    const INCORRECT_PATH: &str = "/path";

    #[test]
    fn create_config_file() {}

    #[test]
    fn config_c8y_create_default() {
        let home = std::env::var("HOME").unwrap();
        let bridge_certfile = format!("{}/.tedge/tedge-certificate.pem", home);
        let bridge_keyfile = format!("{}/.tedge/tedge-private-key.pem", home);
        let expected = Config::C8y(C8yConfig {
            address: "mqtt.latest.stage.c8y.io:8883".into(),
            bridge_certfile,
            bridge_keyfile,
            ..C8yConfig::default()
        });
        assert_eq!(Config::new_c8y().unwrap(), expected);
    }

    #[test]
    fn config_c8y_validate_ok() {
        let cert_file = NamedTempFile::new().unwrap();
        let bridge_certfile = cert_file.path().to_str().unwrap().to_owned();

        let key_file = NamedTempFile::new().unwrap();
        let bridge_keyfile = key_file.path().to_str().unwrap().to_owned();

        let config = Config::C8y(C8yConfig {
            address: CORRECT_URL.into(),
            bridge_certfile,
            bridge_keyfile,
            ..C8yConfig::default()
        });

        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_c8y_validate_wrong_url() {
        let config = Config::C8y(C8yConfig {
            address: INCORRECT_URL.into(),
            bridge_certfile: INCORRECT_PATH.into(),
            bridge_keyfile: INCORRECT_PATH.into(),
            ..C8yConfig::default()
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn config_c8y_validate_wrong_cert_path() {
        let config = Config::C8y(C8yConfig {
            address: CORRECT_URL.into(),
            bridge_certfile: INCORRECT_PATH.into(),
            bridge_keyfile: INCORRECT_PATH.into(),
            ..C8yConfig::default()
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn config_c8y_validate_wrong_key_path() {
        let cert_file = NamedTempFile::new().unwrap();
        let bridge_certfile = cert_file.path().to_str().unwrap().to_owned();

        let config = Config::C8y(C8yConfig {
            address: CORRECT_URL.into(),
            bridge_certfile,
            bridge_keyfile: INCORRECT_PATH.into(),
            ..C8yConfig::default()
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn bridge_config_c8y_create() {
        let mut bridge = C8yConfig::default();

        bridge.bridge_cafile = "./test_root.pem".into();
        bridge.address = "test.test.io:8883".into();
        bridge.bridge_certfile = "./test-certificate.pem".into();
        bridge.bridge_keyfile = "./test-private-key.pem".into();

        let expected = C8yConfig {
            bridge_cafile: "./test_root.pem".into(),
            address: "test.test.io:8883".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            connection: "edge_to_c8y".into(),
            remote_clientid: "alpha".into(),
            try_private: false,
            start_type: "automatic".into(),
            topics: vec![
                // Registration
                r#"s/dcr in 2 c8y/ """#.into(),
                r#"s/ucr out 2 c8y/ """#.into(),
                // Templates
                r#"s/dt in 2 c8y/ """#.into(),
                r#"s/ut/# out 2 c8y/ """#.into(),
                // Static templates
                r#"s/us out 2 c8y/ """#.into(),
                r#"t/us out 2 c8y/ """#.into(),
                r#"q/us out 2 c8y/ """#.into(),
                r#"c/us out 2 c8y/ """#.into(),
                r#"s/ds in 2 c8y/ """#.into(),
                r#"s/os in 2 c8y/ """#.into(),
                // Debug
                r#"s/e in 0 c8y/ """#.into(),
                // SmartRest2
                r#"s/uc/# out 2 c8y/ """#.into(),
                r#"t/uc/# out 2 c8y/ """#.into(),
                r#"q/uc/# out 2 c8y/ """#.into(),
                r#"c/uc/# out 2 c8y/ """#.into(),
                r#"s/dc/# in 2 c8y/ """#.into(),
                r#"s/oc/# in 2 c8y/ """#.into(),
                // c8y JSON
                r#"measurement/measurements/create out 2 c8y/ """#.into(),
                r#"error in 2 c8y/ """#.into(),
            ],
        };

        assert_eq!(bridge, expected);
    }
}
