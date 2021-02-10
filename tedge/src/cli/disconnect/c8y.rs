use std::path::Path;

use structopt::StructOpt;
use tempfile::{NamedTempFile, PersistError};

use crate::command::Command;
use crate::config::{
    ConfigError, TEdgeConfig, C8Y_CONNECT, C8Y_ROOT_CERT_PATH, C8Y_URL, DEVICE_CERT_PATH,
    DEVICE_ID, DEVICE_KEY_PATH, TEDGE_HOME_DIR,
};
use crate::utils::{paths, services};

const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";
const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "bridges";

#[derive(thiserror::Error, Debug)]
pub enum DisconnectError {
    #[error("Bridge connection has not been established, check configuration and try again.")]
    BridgeConnectionFailed,

    #[error("Couldn't load certificate, please provide valid certificate path in configuration.")]
    Certificate,

    #[error("An error occurred in configuration.")]
    Configuration(#[from] ConfigError),

    #[error("Connection cannot be established as config already exists. Please remove existing configuration for the bridge and try again.")]
    ConfigurationExists,

    #[error("Required configuration item is not provided [{item}], run 'tedge config set {item} <value>' to add it to your config.")]
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

    #[error("Path Error: {0}")]
    PathsError(#[from] paths::PathsError),

    #[error("Couldn't write configuration file, ")]
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
pub struct Disconnect {}

impl Command for Disconnect {
    fn to_string(&self) -> String {
        "execute `tedge disconnect`.".into()
    }

    fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        // Ok(self.new_bridge()?)
        Ok(())
    }
}

impl Disconnect {
    fn clean_up(&self) -> Result<(), DisconnectError> {
        let path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            C8Y_CONFIG_FILENAME,
        ])?;
        let _ = std::fs::remove_file(&path).or_else(services::ok_if_not_found)?;

        Ok(())
    }

    fn config_exists(&self) -> Result<(), DisconnectError> {
        let path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            C8Y_CONFIG_FILENAME,
        ])?;

        if Path::new(&path).exists() {
            return Err(DisconnectError::ConfigurationExists);
        }

        Ok(())
    }

    fn save_c8y_config(&self) -> Result<(), DisconnectError> {
        let mut config = TEdgeConfig::from_default_config()?;
        TEdgeConfig::set_config_value(&mut config, C8Y_CONNECT, "true".into())?;
        Ok(TEdgeConfig::write_to_default_config(&config)?)
    }
}

fn get_config_value(config: &TEdgeConfig, key: &str) -> Result<String, DisconnectError> {
    Ok(config
        .get_config_value(key)?
        .ok_or_else(|| DisconnectError::MissingRequiredConfigurationItem { item: key.into() })?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORRECT_URL: &str = "http://test.com";
    const INCORRECT_URL: &str = "noturl";
    const INCORRECT_PATH: &str = "/path";
}
