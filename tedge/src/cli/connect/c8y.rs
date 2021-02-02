use std::path::Path;
use std::time::Duration;

use log;
use structopt::StructOpt;
use tempfile::{NamedTempFile, PersistError};
use tokio::time::timeout;
use url::Url;

use super::utils;
use crate::command::Command;
use mqtt_client::{Client, Message, Topic};

const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";
const C8Y_MQTT_URL: &str = "mqtt.latest.stage.c8y.io:8883";
const DEVICE_CERT_NAME: &str = "tedge-certificate.pem";
const DEVICE_KEY_NAME: &str = "tedge-private-key.pem";
const ROOT_CERT_NAME: &str = "c8y-trusted-root-certificates.pem";
const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "bridges";
const TEDGE_HOME_PREFIX: &str = ".tedge";

#[derive(thiserror::Error, Debug)]
pub enum ConnectError {
    #[error("Bridge connection has not been established, check configuration and try again.")]
    BridgeConnectionFailed,

    #[error("Couldn't load certificate, please provide valid certificate path in configuration.")]
    Certificate,

    #[error("Connection cannot be established as config already exists. Please remove existing configuration for the bridge and try again.")]
    ConfigurationExists,

    #[error("Couldn't load configuration, please provide valid configuration.")]
    InvalidConfiguration(#[from] std::io::Error),

    #[error("MQTT Server is not available on the system, it is required to use this command.")]
    MosquittoNotAvailable,

    #[error("MQTT Server is not available on the system as a service, it is required to use this command.")]
    MosquittoNotAvailableAsService,

    #[error("MQTT Server is active on the system as a service, please stop the service before you use this command.")]
    MosquittoIsActive,

    // #[error("MQTT Server is already running. To create new bridge please stop it using disconnect command.")]
    // MosquittoIsRunning,
    #[error("Couldn't enable MQTT Server. To create new bridge please stop it using disconnect command.")]
    MosquittoCantEnable,

    #[error("MQTT client failed.")]
    MqttClient(#[from] mqtt_client::Error),

    #[error("Couldn't write configutation file, ")]
    PersistError(#[from] PersistError),

    #[error(
        "Systemctl failed. This command requires elevated permissions, please run it with sudo."
    )]
    SystemctlFailed,

    #[error("Systemd is not available on the system or elevated permissions have not been granted, it is required to use this command.")]
    SystemdNotAvailable,

    #[error("Returned error is not recognised.")]
    UnknownReturnCode,

    #[error("Provided endpoint url is not valid, please provide valid url.")]
    UrlParse(#[from] url::ParseError),
}

#[derive(StructOpt, Debug)]
pub struct Connect {}

impl Command for Connect {
    fn to_string(&self) -> String {
        "Connect command creates bridge to selected provider allowing for devices to publish messages via mapper.".into()
    }

    fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        // Awaiting for config story to finish to add this implementation.
        // let config = ConnectCmd::read_configuration();

        self.new_bridge()?;

        Ok(())
    }
}

impl Connect {
    fn new_bridge(&self) -> Result<(), ConnectError> {
        // // println!("{:?}", utils::build_path_from_home(&["abc"]));
        log::info!("Checking if systemd and mosquitto are available.");
        let _ = utils::all_services_available()?;
        log::debug!("Systemd and mosquitto are available.");

        log::info!("Checking if configuration for requested bridge already exists.");
        let _ = self.config_exists()?;
        log::debug!("Configuration for requested bridge already exists.");

        log::info!("Checking configuration for requested bridge.");
        let config = self.load_config_with_validatation()?;
        log::debug!("Cconfiguration for requested bridge is valid.");

        log::info!("Creating configuration for requested bridge.");
        // Need to use home for now.
        let bridge_config = BridgeConf::from_config(config)?;
        match self.write_bridge_config_to_file(&bridge_config) {
            Err(err) => {
                log::error!("{:?}", err);
                self.clean_up()?;
                return Err(err);
            }
            _ => {}
        }

        match utils::mosquitto_restart_daemon() {
            Err(err) => {
                log::error!("{:?}", err);
                self.clean_up()?;
                return Err(err);
            }
            _ => {}
        }

        // Error if cloud not available (send mqtt message to validate connection)
        // match self.check_connection() {
        //     Err(err) => {
        //         log::error!("{:?}", err);
        //         self.clean_up()?;
        //         return Err(err);
        //     }
        //     _ => {}
        // }

        match utils::mosquitto_enable_daemon() {
            Err(err) => {
                self.clean_up()?;
                return Err(err);
            }
            _ => {}
        }

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

        let _error_handle = tokio::spawn(async move {
            while let Some(message) = template_response.next().await {
                if std::str::from_utf8(&message.payload)
                    .unwrap_or("")
                    .contains("41,notExistingTemplateCollection")
                {
                    let _ = sender.send(true);
                    println!("here");
                    break;
                }
            }
        });

        self.publish_test_message(mqtt, template_pub_topic).await?;

        let fut = timeout(Duration::from_secs(WAIT_FOR_SECONDS), receiver);

        match fut.await {
            Ok(Ok(true)) => {
                log::debug!("Received message.");
            }
            _ => {
                return Err(ConnectError::BridgeConnectionFailed);
            }
        }

        log::info!("Successully created bridge connection!");
        Ok(())
    }

    async fn publish_test_message(
        &self,
        mqtt: Client,
        c8y_msg: Topic,
    ) -> Result<(), mqtt_client::Error> {
        mqtt.publish(Message::new(&c8y_msg, "")).await?;
        mqtt.disconnect().await?;

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

    fn load_config_with_validatation(&self) -> Result<Config, ConnectError> {
        Config::new_c8y().validate()
    }

    fn write_bridge_config_to_file(&self, config: &BridgeConf) -> Result<(), ConnectError> {
        let mut temp_file = NamedTempFile::new()?;
        let _ = config.serialize(&mut temp_file)?;

        let dir_path =
            utils::build_path_from_home(&[TEDGE_HOME_PREFIX, TEDGE_BRIDGE_CONF_DIR_PATH])?;

        // This will forcefully create directory structure if doessn't exist, we should find better way to do it, maybe config should deal with it?
        let _ = std::fs::create_dir_all(dir_path)?;

        let config_path = utils::build_path_from_home(&[
            TEDGE_HOME_PREFIX,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            C8Y_CONFIG_FILENAME,
        ])?;

        println!("{}", &config_path);

        temp_file.persist(config_path)?;

        Ok(())
    }
}

#[derive(Debug, PartialEq)]
enum Config {
    C8y(C8yConfig),
}

impl Config {
    fn new_c8y() -> Config {
        Config::C8y(C8yConfig::default())
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
    url: String,
    cert_path: String,
    key_path: String,
    bridge_config: BridgeConf,
}

impl Default for C8yConfig {
    fn default() -> Self {
        let cert_path =
            utils::build_path_from_home(&[TEDGE_HOME_PREFIX, DEVICE_CERT_NAME]).unwrap_or_default();
        println!("{}", &cert_path);

        let key_path =
            utils::build_path_from_home(&[TEDGE_HOME_PREFIX, DEVICE_KEY_NAME]).unwrap_or_default();

        C8yConfig {
            url: C8Y_MQTT_URL.into(),
            cert_path,
            key_path,
            bridge_config: BridgeConf::default(),
        }
    }
}

impl C8yConfig {
    fn validate(&self) -> Result<(), ConnectError> {
        Url::parse(&self.url)?;

        if !Path::new(&self.cert_path).exists() {
            return Err(ConnectError::Certificate);
        }

        if !Path::new(&self.key_path).exists() {
            return Err(ConnectError::Certificate);
        }

        Ok(())
    }
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

#[derive(Debug, PartialEq, Eq)]
struct BridgeConf {
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

impl Default for BridgeConf {
    fn default() -> Self {
        BridgeConf {
            connection: "edge_to_c8y".into(),
            address: "".into(),
            bridge_cafile: "".into(),
            remote_clientid: "".into(),
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

impl BridgeConf {
    /// Validates provider configuration as per required parameters
    /// E.g. c8y requires following parameters to create bridge:
    ///  - url (endpoint url to publish messages)
    ///  - cert_path (path to device certificate)
    ///  - key_path (path to device private key)
    // Look at error type, maybe parseerror
    fn from_config(config: Config) -> Result<BridgeConf, ConnectError> {
        match config {
            Config::C8y(config) => Ok(BridgeConf {
                bridge_cafile: utils::build_path_from_home(&[TEDGE_HOME_PREFIX, ROOT_CERT_NAME])?,
                address: config.url.into(),
                bridge_certfile: config.cert_path.into(),
                bridge_keyfile: config.key_path.into(),
                ..BridgeConf::default()
            }),
        }
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
        let cert_path = format!("{}/.tedge/tedge-certificate.pem", home);
        let key_path = format!("{}/.tedge/tedge-private-key.pem", home);
        let expected = Config::C8y(C8yConfig {
            url: "mqtt.latest.stage.c8y.io:8883".into(),
            cert_path,
            key_path,
            bridge_config: BridgeConf::default(),
        });
        assert_eq!(Config::new_c8y(), expected);
    }

    #[test]
    fn config_c8y_validate_ok() {
        let cert_file = NamedTempFile::new().unwrap();
        let cert_path = cert_file.path().to_str().unwrap().to_owned();

        let key_file = NamedTempFile::new().unwrap();
        let key_path = key_file.path().to_str().unwrap().to_owned();

        let config = Config::C8y(C8yConfig {
            url: CORRECT_URL.into(),
            cert_path,
            key_path,
            bridge_config: BridgeConf::default(),
        });

        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_c8y_validate_wrong_url() {
        let config = Config::C8y(C8yConfig {
            url: INCORRECT_URL.into(),
            cert_path: INCORRECT_PATH.into(),
            key_path: INCORRECT_PATH.into(),
            bridge_config: BridgeConf::default(),
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn config_c8y_validate_wrong_cert_path() {
        let config = Config::C8y(C8yConfig {
            url: CORRECT_URL.into(),
            cert_path: INCORRECT_PATH.into(),
            key_path: INCORRECT_PATH.into(),
            bridge_config: BridgeConf::default(),
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn config_c8y_validate_wrong_key_path() {
        let cert_file = NamedTempFile::new().unwrap();
        let cert_path = cert_file.path().to_str().unwrap().to_owned();

        let config = Config::C8y(C8yConfig {
            url: CORRECT_URL.into(),
            cert_path,
            key_path: INCORRECT_PATH.into(),
            bridge_config: BridgeConf::default(),
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn bridge_config_c8y_create() {
        let mut bridge = BridgeConf::default();

        bridge.bridge_cafile = "./test_root.pem".into();
        bridge.address = "test.test.io:8883".into();
        bridge.bridge_certfile = "./test-certificate.pem".into();
        bridge.bridge_keyfile = "./test-private-key.pem".into();

        let expected = BridgeConf {
            bridge_cafile: "./test_root.pem".into(),
            address: "test.test.io:8883".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            connection: "edge_to_c8y".into(),
            remote_clientid: "".into(),
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
