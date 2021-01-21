use crate::command::Command;
use serde::{Deserialize, Serialize};

use serde::ser::{SerializeStruct, Serializer};
use structopt::StructOpt;
use url::Url;

#[derive(StructOpt, Debug)]
pub struct Connect {}

impl Command for Connect {
    fn to_string(&self) -> String {
        String::new()
    }

    fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        // let config = ConnectCmd::read_configuration();
        match self {
            Connect {} => Connect::new()?,
        };
        Ok(())
    }
}

mod utils {
    // How about using some crates like for example 'which'
    pub fn systemd_available() -> bool {
        std::process::Command::new("systemctl")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    pub fn mosquitto_available() -> bool {
        std::process::Command::new("mosquitto")
            .arg("-h")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()
            .map(|status| status.code() == Some(3))
            .unwrap_or(false)
    }

    pub fn mosquitto_available_as_service() -> bool {
        std::process::Command::new("systemctl")
            .arg("status")
            .arg("mosquitto")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()
            .map(|status| status.code() == Some(3))
            .unwrap_or(false)
    }

    pub fn mosquitto_is_active_daemon() -> bool {
        std::process::Command::new("systemctl")
            .arg("is-active")
            .arg("mosquitto.service")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()
            .map(|status| status.code() == Some(3))
            .unwrap_or(false)
    }
}

impl Connect {
    fn new() -> Result<(), ConnectError> {
        // Check mosquitto service is available
        // 1. check if systemd is available
        // 2. check if mosquitto exists as a service
        // 3. check if mosquitto is up
        //
        // This is just quick and naive way to check if systemd is available,
        // we should most likely find a better way to perform this check.
        // if !utils::systemd_available() {
        //     return Err(ConnectError::SystemdUnavailableError {});
        // }

        // Check mosquitto exists on the system
        if !utils::mosquitto_available() {
            return Err(ConnectError::MosquittoNotAvailableError {});
        }

        // Check mosquitto mosquitto available through systemd
        // Theoretically we could just do a big boom and run just this command as it will error on following:
        //  - systemd not available
        //  - mosquitto not install as a service
        // That for instance would be sufficient and would return an error anyway, but I prefer to do it gently with separate checks.
        if !utils::mosquitto_available_as_service() {
            return Err(ConnectError::MosquittoNotAvailableAsServiceError {});
        }

        // Check mosquitto is running
        if !utils::mosquitto_is_active_daemon() {
            return Err(ConnectError::MosquittoNotAvailableAsServiceError {});
        };

        // Check connected (c8y-bridge.conf present) // fail if so
        if Self::config_exists() {
            return Err(ConnectError::ConfigurationExistsError {});
        }

        // Check configuration for provider is provided and correct // otherwise fail with error
        // awaits config from Albin let's hardcode values for now
        // Check current configuration to make sure that the current provider is not connected.
        let config = Self::load_config()?;

        // Verify current config does not contain just loaded config
        // This check may not be required as the config_exists does similar check

        // Create mosquitto config with relay and place it in /etc/whatever

        let bridge_config = Self::generate_bridge_config(&config)?;

        println!("{}", bridge_config);
        // Check configuration is correct and restart mosquitto

        // if mosq

        // Error if cloud not available (send mqtt message to validate connection)

        // Clean up

        // Terminate if something goes wrong
        // Unit tests
        Ok(())
    }

    fn config_exists() -> bool {
        std::path::Path::new("./c8y-bridge.conf").exists()
    }

    fn load_config() -> Result<Config, ConnectError> {
        // Config::new_c8y().validate()
        Ok(Config::new_c8y())
    }

    /// Validates provider configuration as per required parameters
    /// E.g. c8y requires following parameters to create relay:
    ///  - url (endpoint url to publish messages)
    ///  - cert_path (path to device certificate)
    ///  - key_path (path to device private key)
    // Look at error type, maybe parseerror
    fn generate_bridge_config(config: &Config) -> Result<BridgeConf, ConnectError> {
        let mut bridge = BridgeConf::default();

        bridge.bridge_cafile = String::from("./c8y-trusted-root-certificates.pem");

        match config {
            Config::C8y(config) => {
                bridge.address = config.url.to_owned();
                bridge.bridge_certfile = config.cert_path.to_owned();
                bridge.bridge_keyfile = config.key_path.to_owned();
            }
        }

        Ok(bridge)
    }
}

#[derive(thiserror::Error, Debug, Eq, PartialEq)]
enum ConnectError {
    #[error("Connection cannot be established as config already exists.")]
    ConfigurationExistsError {},

    // #[error("Couldn't load configuration, please provide valid configuration.")]
    // InvalidConfigurationError {},
    #[error("Couldn't load certificate, provide valid certificate.")]
    CertificateError {},

    #[error("Provided endpoint url is not valid, please provide valid url.")]
    UrlParseError(#[from] url::ParseError),

    #[error("Systemd is not available on the system, it is required to use this command.")]
    SystemdUnavailableError {},

    #[error("Mosquitto is not available on the system, it is required to use this command.")]
    MosquittoNotAvailableError {},

    #[error("Mosquitto is not available on the system as a service, it is required to use this command.")]
    MosquittoNotAvailableAsServiceError {},
}

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

        bridge.bridge_cafile = String::from("./c8y-trusted-root-certificates.pem");

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

#[derive(Debug)]
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
            return Err(ConnectError::CertificateError {});
        }

        if !std::path::Path::new(&self.key_path).exists() {
            return Err(ConnectError::CertificateError {});
        }

        Ok(())
    }
}

/// # C8Y Bridge
/// connection edge_to_c8y
/// address mqtt.$C8Y_URL:8883
/// bridge_cafile $C8Y_CERT
/// remote_clientid $DEVICE_ID
/// bridge_certfile $CERT_PATH
/// bridge_keyfile $KEY_PATH
/// try_private false
/// start_type automatic

#[derive(Serialize, Deserialize, Debug)]
struct BridgeConf {
    connection: String,
    address: String,
    bridge_cafile: String,
    remote_clientid: String,
    bridge_certfile: String,
    bridge_keyfile: String,
    try_private: bool,
    start_type: String,
}

impl Default for BridgeConf {
    fn default() -> Self {
        BridgeConf {
            connection: "".into(),
            address: "".into(),
            bridge_cafile: "".into(),
            remote_clientid: "".into(),
            bridge_certfile: "".into(),
            bridge_keyfile: "".into(),
            try_private: false,
            start_type: "automatic".into(),
        }
    }
}

use std::fmt;
use std::fmt::{Debug, Display, Formatter};

impl std::fmt::Display for BridgeConf {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            r#"connection {}
address {}
bridge_cafile {}
remote_clientid {}
bridge_certfile {}
bridge_keyfile {}
try_private {}
start_type {}
"#,
            self.connection,
            self.address,
            self.bridge_cafile,
            self.remote_clientid,
            self.bridge_certfile,
            self.bridge_keyfile,
            self.try_private,
            self.start_type
        )
    }
}
// impl Serialize for BridgeConf2 {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         // let key = |k1, k2| format!("({},{})", k1, k2);
//         // 2 is the number of fields in the struct.
//         let mut state = serializer.serialize_struct("BridgeConf2", 2)?;
//         state.serialize_field("connection", &self.connection)?;
//         state.serialize_field("address", &self.address)?;
//         state.end()
//     }
// }

// pub fn to_string<T>(value: &T) -> Result<String>
// where
//     T: ?Sized + Serialize,
// {
//     let vec = tri!(to_vec(value));
//     let string = unsafe {
//         // We do not emit invalid UTF-8.
//         String::from_utf8_unchecked(vec)
//     };
//     Ok(string)
// }
