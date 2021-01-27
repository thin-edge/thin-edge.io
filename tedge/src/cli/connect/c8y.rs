use log;
use structopt::StructOpt;
use url::Url;

use super::utils;
use crate::command::Command;
use mqtt_client::{Client, Message, Topic};

const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";
const ROOT_CERT_NAME: &str = "c8y-trusted-root-certificates.pem";
const TEDGE_HOME_PREFIX: &str = ".tedge";

#[derive(StructOpt, Debug)]
pub struct Connect {}

impl Command for Connect {
    fn to_string(&self) -> String {
        String::from("Connect command creates relay to selected provider allowing for devices to publish messages via mapper.")
    }

    fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        // Awaiting for config story to finish to add this implementation.
        // let config = ConnectCmd::read_configuration();

        match self {
            Connect {} => Connect::new_relay()?,
        };

        Ok(())
    }
}

impl Connect {
    fn new_relay() -> Result<(), ConnectError> {
        utils::all_services_available()?;

        // Check connected (c8y-bridge.conf present) // fail if so
        Self::config_exists()?;

        // Check configuration for provider is provided and correct // otherwise fail with error
        // awaits config from Albin let's hardcode values for now
        // Check current configuration to make sure that the current provider is not connected.
        // This needs to cleanup after error...
        let config = Self::load_config_with_validatation()?;

        // Verify current config does not contain just loaded config
        // This check may not be required as the config_exists does similar check
        // Create mosquitto config with relay and place it in /etc/whatever
        // Need to use home for now.
        let bridge_config = Self::generate_bridge_config(&config)?;
        Self::write_bridge_config_to_file(&bridge_config)?;

        // Check configuration is correct and restart mosquitto
        utils::mosquitto_restart_daemon()?;

        // Error if cloud not available (send mqtt message to validate connection)
        Self::check_connection()?;

        // Self::clean_up();

        Ok(())
    }

    fn clean_up() -> Result<(), ConnectError> {
        fn ok_if_not_found(err: std::io::Error) -> std::io::Result<()> {
            match err.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(err),
            }
        }

        let home_dir = utils::home_dir().ok_or(ConnectError::ConfigurationExists)?;

        // Check if config file exists
        let path = format!(
            "{:?}/{}/{}",
            home_dir, TEDGE_HOME_PREFIX, C8Y_CONFIG_FILENAME
        );

        if std::path::Path::new(&path).exists() {
            std::fs::remove_file(&path).or_else(ok_if_not_found)?;
        }

        Ok(())
    }

    #[tokio::main]
    async fn check_connection() -> Result<(), ConnectError> {
        const WAIT_FOR_SECONDS: u64 = 5;

        let c8y_msg = Topic::new("c8y/s/us")?;
        let c8y_err = Topic::new("c8y/s/e")?;

        let mqtt = Client::connect("connection_test", &mqtt_client::Config::default()).await?;
        let mut c8y_errors = mqtt.subscribe(c8y_err.filter()).await?;

        let (sender, receiver) = tokio::sync::oneshot::channel();

        let _error_handle = tokio::spawn(async move {
            while let Some(message) = c8y_errors.next().await {
                if std::str::from_utf8(&message.payload)
                    .unwrap_or("")
                    .contains("999,No static template")
                {
                    let _ = sender.send(true);
                    break;
                }
            }
        });

        Self::publish_temperature(mqtt, c8y_msg).await?;

        let fut = tokio::time::timeout(std::time::Duration::from_secs(WAIT_FOR_SECONDS), receiver);

        match fut.await {
            Ok(Ok(true)) => {
                println!("Got message");
            }
            _ => {}
        }

        Ok(())
    }

    async fn publish_temperature(mqtt: Client, c8y_msg: Topic) -> Result<(), mqtt_client::Error> {
        let payload = format!("{},{}", "999", 999);
        log::debug!("{}", payload);
        mqtt.publish(Message::new(&c8y_msg, payload)).await?;

        futures_timer::Delay::new(std::time::Duration::from_millis(1000)).await;

        mqtt.disconnect().await?;
        Ok(())
    }

    fn config_exists() -> Result<(), ConnectError> {
        let home_dir = utils::home_dir().ok_or(ConnectError::ConfigurationExists)?;

        let path = format!(
            "{:?}/{}/{}",
            home_dir, TEDGE_HOME_PREFIX, C8Y_CONFIG_FILENAME
        );

        if !std::path::Path::new(&path).exists() {
            return Err(ConnectError::ConfigurationExists);
        }

        Ok(())
    }

    fn load_config_with_validatation() -> Result<Config, ConnectError> {
        Config::new_c8y().validate()
    }

    /// Validates provider configuration as per required parameters
    /// E.g. c8y requires following parameters to create relay:
    ///  - url (endpoint url to publish messages)
    ///  - cert_path (path to device certificate)
    ///  - key_path (path to device private key)
    ///  - bridge_cafile
    // Look at error type, maybe parseerror
    fn generate_bridge_config(config: &Config) -> Result<BridgeConf, ConnectError> {
        let mut bridge = BridgeConf::default();

        let home_dir = utils::home_dir().ok_or(ConnectError::ConfigurationExists)?;

        bridge.bridge_cafile = String::from(format!(
            "{:?}/{}/{}",
            home_dir, TEDGE_HOME_PREFIX, ROOT_CERT_NAME
        ));

        match config {
            Config::C8y(config) => {
                bridge.address = config.url.to_owned();
                bridge.bridge_certfile = config.cert_path.to_owned();
                bridge.bridge_keyfile = config.key_path.to_owned();
            }
        }

        Ok(bridge)
    }

    // write_all may fail, let's have a look how to overcome it?
    // Maybe AtomicWrite??
    fn write_bridge_config_to_file(config: &BridgeConf) -> Result<(), ConnectError> {
        let home_dir = utils::home_dir().ok_or(ConnectError::ConfigurationExists)?;

        let mut file = std::fs::File::create(format!(
            "{:?}/{}/{}",
            home_dir, TEDGE_HOME_PREFIX, C8Y_CONFIG_FILENAME
        ))?;
        config.serialize(&mut file)?;
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ConnectError {
    #[error("Connection cannot be established as config already exists.")]
    ConfigurationExists,

    #[error("Couldn't load configuration, please provide valid configuration.")]
    InvalidConfigurationError(#[from] std::io::Error),

    #[error("Couldn't load certificate, provide valid certificate.")]
    Certificate,

    #[error("Provided endpoint url is not valid, please provide valid url.")]
    UrlParse(#[from] url::ParseError),

    #[error("Systemd is not available on the system, it is required to use this command.")]
    SystemdUnavailable,

    #[error("Mosquitto is not available on the system, it is required to use this command.")]
    MosquittoNotAvailable,

    #[error("Mosquitto is not available on the system as a service, it is required to use this command.")]
    MosquittoNotAvailableAsService,

    #[error("MQTT Server is already running. To create new relay please stop it using disconnect command.")]
    MosquittoRunning,

    #[error("Systemctl failed: `{reason:?}`")]
    SystemctlFailed { reason: String },

    #[error("Mosquitto failed: `{reason:?}`")]
    MosquittoFailed { reason: String },

    #[error("MQTT client failed.")]
    MqttClient(#[from] mqtt_client::Error),
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

    /// Validates provider configuration as per required parameters
    /// E.g. c8y requires following parameters to create relay:
    ///  - url (endpoint url to publish messages)
    ///  - cert_path (path to device certificate)
    ///  - key_path (path to device private key)
    // Look at error type, maybe parseerror
    fn generate_bridge_config(self) -> Result<BridgeConf, ConnectError> {
        let mut bridge = BridgeConf::default();

        bridge.bridge_cafile = String::from(ROOT_CERT_NAME);

        match self {
            Config::C8y(config) => {
                bridge.address = config.url.to_owned();
                bridge.bridge_certfile = config.cert_path.to_owned();
                bridge.bridge_keyfile = config.key_path.to_owned();
            }
        }

        Ok(bridge)
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
        C8yConfig {
            url: String::from("mqtt.latest.stage.c8y.io:8883"),
            cert_path: String::from("./tedge-certificate.pem"),
            key_path: String::from("./tedge-private-key.pem"),
            bridge_config: BridgeConf::default(),
        }
    }
}

impl C8yConfig {
    fn validate(&self) -> Result<(), ConnectError> {
        Url::parse(&self.url)?;

        if !std::path::Path::new(&self.cert_path).exists() {
            return Err(ConnectError::Certificate);
        }

        if !std::path::Path::new(&self.key_path).exists() {
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

#[derive(Debug, PartialEq)]
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
    use tempfile::*;

    const CORRECT_URL: &str = "http://test.com";
    const INCORRECT_URL: &str = "noturl";
    const INCORRECT_PATH: &str = "/path";

    #[test]
    fn create_config_file() {}

    #[test]
    fn config_c8y_create() {
        let expected = Config::C8y(C8yConfig {
            url: "mqtt.latest.stage.c8y.io:8883".into(),
            cert_path: "./tedge-certificate.pem".into(),
            key_path: "./tedge-private-key.pem".into(),
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

    // #[test]
    // fn bridge_config_c8y_create() {
    //     let mut bridge = BridgeConf::default();

    //     bridge.bridge_cafile = String::from("./test_root.pem");
    //     bridge.address = String::from("test.test.io:8883");
    //     bridge.bridge_certfile = String::from("./test-certificate.pem");
    //     bridge.bridge_keyfile = String::from("./test-private-key.pem");

    //     let expected = BridgeConf {
    //         bridge_cafile: "./test_root.pem".into(),
    //         address: "test.test.io:8883".into(),
    //         bridge_certfile: "./test-certificate.pem".into(),
    //         bridge_keyfile: "./test-private-key.pem".into(),
    //     };

    //     assert_eq!(bridge.to_string(), expected.to_string());
    // }
}
