use std::fs::File;
use std::io::prelude::*;

use futures::future::FutureExt;
use futures::select;
use futures_timer::Delay;
use log;
use rand::prelude::*;
use std::time::Duration;
use structopt::StructOpt;
use url::Url;

use crate::command::Command;
use mqtt_client::{Client, ErrorStream, Message, MessageStream, Topic};

// mod utils;
// use crate::utils;

const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";

#[derive(StructOpt, Debug)]
pub struct Connect {}

impl Command for Connect {
    fn to_string(&self) -> String {
        String::new()
    }

    fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        // Awaiting for config story to finish to add this implementation.
        // let config = ConnectCmd::read_configuration();

        match self {
            Connect {} => Connect::create_relay()?,
        };

        Ok(())
    }
}

mod utils {
    use std::env;
    use std::path::PathBuf;

    use super::ConnectError;

    enum MosquittoCmd {
        Base,
        Status,
    }

    impl MosquittoCmd {
        fn as_str(self) -> &'static str {
            match self {
                MosquittoCmd::Base => "mosquitto",
                MosquittoCmd::Status => "-h",
            }
        }
    }

    enum SystemCtlCmd {
        Base,
        IsActive,
        Restart,
        Status,
        Version,
    }

    impl SystemCtlCmd {
        fn as_str(self) -> &'static str {
            match self {
                SystemCtlCmd::Base => "systemctl",
                SystemCtlCmd::IsActive => "is-active",
                SystemCtlCmd::Restart => "restart",
                SystemCtlCmd::Status => "status",
                SystemCtlCmd::Version => "--version",
            }
        }
    }

    type ExitCode = i32;
    enum ExitCodes {}

    impl ExitCodes {
        pub const MOSQUITTOCMD_SUCCESS: ExitCode = 3;
        pub const SUCCESS: ExitCode = 0;
        pub const SYSTEMCTL_ISACTICE_SUCCESS: ExitCode = 3;
        pub const SYSTEMCTL_STATUS_SUCCESS: ExitCode = 3;
    }

    // This isn't complete way to retrieve HOME dir from the user.
    // We could parse passwd file to get actual home path if we can get user name.
    // I suppose rust provides some way to do it or allows through c bindings... But this implies unsafe code.
    // Another alternative is to use deprecated env::home_dir() -1
    // https://github.com/rust-lang/rust/issues/71684
    pub fn home_dir() -> Option<PathBuf> {
        return env::var_os("HOME")
            .and_then(|home| if home.is_empty() { None } else { Some(home) })
            // .or_else(|| return None; )
            .map(PathBuf::from);
    }

    // Another simple method which has now been deprecated.
    // (funny, advice says look on crates.io two of crates supposedly do what is expected are not necessarily correct:
    // one uses unsafe code and another uses this method with deprecated env call)
    pub fn home_dir2() -> Option<PathBuf> {
        #[allow(deprecated)]
        std::env::home_dir()
    }

    // How about using some crates like for example 'which'??
    pub fn systemd_available() -> Result<bool, ConnectError> {
        std::process::Command::new(SystemCtlCmd::Base.as_str())
            .arg(SystemCtlCmd::Version.as_str())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_or_else(
                |err| Err(ConnectError::SystemdUnavailable),
                |status| Ok(status.success()),
            )
    }

    pub fn mosquitto_available() -> Result<bool, ConnectError> {
        match mosquitto_cmd_nostd(MosquittoCmd::Status.as_str(), 3) {
            true => Ok(true),
            false => Err(ConnectError::MosquittoNotAvailable),
        }
    }

    pub fn mosquitto_available_as_service() -> Result<bool, ConnectError> {
        match systemctl_cmd_nostd(
            SystemCtlCmd::Status.as_str(),
            MosquittoCmd::Base.as_str(),
            ExitCodes::SYSTEMCTL_STATUS_SUCCESS,
        ) {
            true => Ok(true),
            false => Err(ConnectError::MosquittoNotAvailableAsService),
        }
    }

    pub fn mosquitto_is_active_daemon() -> Result<bool, ConnectError> {
        systemctl_is_active_nostd(MosquittoCmd::Base.as_str(), ExitCodes::MOSQUITTOCMD_SUCCESS)
    }

    // Note that restarting a unit with this command does not necessarily flush out all of the unit's resources before it is started again.
    // For example, the per-service file descriptor storage facility (see FileDescriptorStoreMax= in systemd.service(5)) will remain intact
    // as long as the unit has a job pending, and is only cleared when the unit is fully stopped and no jobs are pending anymore.
    // If it is intended that the file descriptor store is flushed out, too, during a restart operation an explicit
    // systemctl stop command followed by systemctl start should be issued.
    pub fn mosquitto_restart_daemon() -> Result<(), ConnectError> {
        match systemctl_restart_nostd(
            MosquittoCmd::Base.as_str(),
            ExitCodes::SYSTEMCTL_ISACTICE_SUCCESS,
        ) {
            Ok(_) => Ok(()),
            Err(_) => Err(ConnectError::MosquittoNotAvailableAsService),
        }
    }

    fn systemctl_is_active_nostd(service: &str, expected_code: i32) -> Result<bool, ConnectError> {
        match systemctl_cmd_nostd(SystemCtlCmd::IsActive.as_str(), service, expected_code) {
            true => Ok(true),
            false => Err(ConnectError::SystemctlFailed {
                reason: format!("Service '{}' is not active", service).into(),
            }),
        }
    }

    fn systemctl_restart_nostd(service: &str, expected_code: i32) -> Result<bool, ConnectError> {
        match systemctl_cmd_nostd(SystemCtlCmd::Restart.as_str(), service, expected_code) {
            true => Ok(true),
            false => Err(ConnectError::SystemctlFailed {
                reason: "Restart required service {service}".into(),
            }),
        }
    }

    fn systemctl_cmd_nostd(cmd: &str, service: &str, expected_code: i32) -> bool {
        std::process::Command::new(SystemCtlCmd::Base.as_str())
            .arg(cmd)
            .arg(service)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()
            .map(|status| status.code() == Some(expected_code))
            .unwrap_or(false)
    }

    fn mosquitto_cmd_nostd(cmd: &str, expected_code: i32) -> bool {
        std::process::Command::new(MosquittoCmd::Base.as_str())
            .arg(cmd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()
            .map(|status| status.code() == Some(expected_code))
            .unwrap_or(false)
    }
}

impl Connect {
    fn create_relay() -> Result<(), ConnectError> {
        // Check mosquitto service is available
        // 1. check if systemd is available
        // 2. check if mosquitto exists as a service
        // 3. check if mosquitto is up

        // Check all required parameters are set
        // if !Self::config_ok() {
        //     return Err(ConnectError::ConfigurationParatemers);
        // }

        // This is just quick and naive way to check if systemd is available,
        // we should most likely find a better way to perform this check.
        utils::systemd_available()?;

        // Check mosquitto exists on the system
        utils::mosquitto_available()?;

        // Check mosquitto mosquitto available through systemd
        // Theoretically we could just do a big boom and run just this command as it will error on following:
        //  - systemd not available
        //  - mosquitto not installed as a service
        // That for instance would be sufficient and would return an error anyway, but I prefer to do it gently with separate checks.
        utils::mosquitto_available_as_service()?;

        // Check mosquitto is running
        utils::mosquitto_is_active_daemon()?;

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

        // Clean up

        // Add error handling here.
        // Self::clean_up();

        // Terminate if something goes wrong
        // Unit tests
        Ok(())
    }

    fn clean_up() -> Result<(), ConnectError> {
        fn ok_if_not_found(err: std::io::Error) -> std::io::Result<()> {
            match err.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(err),
            }
        }

        // Check if config file exists
        let path = format!(
            "{:?}/.tedge/{}",
            utils::home_dir().unwrap_or(std::path::PathBuf::from(".")),
            C8Y_CONFIG_FILENAME
        );

        if std::path::Path::new(&path).exists() {
            std::fs::remove_file(&path).or_else(ok_if_not_found)?;
        }
        // Remove config file if exists if conenction was unsuccessful
        // Shutdown mosquitto
        // Do I need to return anything here>/#
        Ok(())
    }

    // timeout (5 seconds??) on error
    #[tokio::main]
    async fn check_connection() -> Result<(), ConnectError> {
        let c8y_msg = Topic::new("c8y/s/us")?;
        let c8y_err = Topic::new("c8y/s/e")?;

        env_logger::init();

        let mqtt = Client::connect("connection_test", &mqtt_client::Config::default()).await?;
        let mut c8y_errors = mqtt.subscribe(c8y_err.filter()).await?;

        Self::publish_temperature(mqtt, c8y_msg).await?;
        while let Some(message) = c8y_errors.next().await {
            log::error!("C8Y error: {:?}", message.payload);
        }
        Ok(())
    }

    async fn publish_temperature(mqtt: Client, c8y_msg: Topic) -> Result<(), mqtt_client::Error> {
        let payload = format!("{},{}", "999", 999);
        log::debug!("{}", payload);
        mqtt.publish(Message::new(&c8y_msg, payload)).await?;

        Delay::new(Duration::from_millis(1000)).await;

        mqtt.disconnect().await?;
        Ok(())
    }

    fn random_in_range(low: i32, high: i32) -> i32 {
        let mut rng = thread_rng();
        rng.gen_range(low..high)
    }

    async fn listen_c8y_error(mut messages: MessageStream) {
        while let Some(message) = messages.next().await {
            log::error!("C8Y error: {:?}", message.payload);
        }
    }

    async fn listen_error(mut errors: ErrorStream) {
        while let Some(error) = errors.next().await {
            log::error!("System error: {}", error);
        }
    }

    fn config_exists() -> Result<(), ConnectError> {
        let path = format!(
            "{:?}/.tedge/{}",
            utils::home_dir().unwrap_or(std::path::PathBuf::from(".")),
            C8Y_CONFIG_FILENAME
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

        bridge.bridge_cafile = String::from(format!(
            "{:?}/.tedge/c8y-trusted-root-certificates.pem",
            utils::home_dir().unwrap_or(std::path::PathBuf::from("."))
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
        let mut file = File::create(format!(
            "{:?}/.tedge/{}",
            utils::home_dir().unwrap_or(std::path::PathBuf::from(".")),
            C8Y_CONFIG_FILENAME
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
            connection: "".into(),
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
        writeln!(writer, "connection {}", self.connection)?;
        writeln!(writer, "address {}", self.address)?;
        writeln!(writer, "bridge_cafile {}", self.bridge_cafile)?;
        writeln!(writer, "remote_clientid {}", self.remote_clientid)?;
        writeln!(writer, "bridge_certfile {}", self.bridge_certfile)?;
        writeln!(writer, "bridge_keyfile {}", self.bridge_keyfile)?;
        writeln!(writer, "try_private {}", self.try_private)?;
        writeln!(writer, "start_type {}", self.start_type)?;

        for topic in &self.topics {
            writeln!(writer, "topic {}", topic)?;
        }

        Ok(())
    }
}

struct TopicsList {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::*;

    #[test]
    fn create_config_file() {}

    #[test]
    fn config_c8y_create() {
        let expected = Config::C8y(C8yConfig {
            url: "url".into(),
            cert_path: "path".into(),
            key_path: "path".into(),
            bridge_config: BridgeConf::default(),
        });
        assert_eq!(Config::new_c8y(), expected);
    }

    #[test]
    fn config_c8y_validate_ok() {
        let config = Config::C8y(C8yConfig {
            url: "test.com".into(),
            cert_path: "/path".into(),
            key_path: "/path".into(),
            bridge_config: BridgeConf::default(),
        });
        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_c8y_validate_wrong_url() {
        let config = Config::C8y(C8yConfig {
            url: "noturl".into(),
            cert_path: "/path".into(),
            key_path: "/path".into(),
            bridge_config: BridgeConf::default(),
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn config_c8y_validate_wrong_cert_path() {
        let config = Config::C8y(C8yConfig {
            url: "test.com".into(),
            cert_path: "/path".into(),
            key_path: "/path".into(),
            bridge_config: BridgeConf::default(),
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn config_c8y_validate_wrong_key_path() {
        let config = Config::C8y(C8yConfig {
            url: "test.com".into(),
            cert_path: "/path".into(),
            key_path: "/path".into(),
            bridge_config: BridgeConf::default(),
        });

        assert!(config.validate().is_err());
    }
    #[test]
    fn bridge_config_c8y_create() {
        let mut bridge = BridgeConf::default();

        bridge.bridge_cafile = String::from("./test_root.pem");
        bridge.address = String::from("test.test.io:8883");
        bridge.bridge_certfile = String::from("./test-certificate.pem");
        bridge.bridge_keyfile = String::from("./test-private-key.pem");

        let expected = BridgeConf {
            bridge_cafile: "./test_root.pem".into(),
            address: "test.test.io:8883".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
        };

        assert_eq!(bridge.to_string(), expected.to_string());
    }

    // #[test]
    // fn create_config_file() {}
    // #[test]
    // fn create_config_file() {}
    // #[test]
    // fn create_config_file() {}
}
