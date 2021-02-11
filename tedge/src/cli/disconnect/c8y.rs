use std::path::Path;

use structopt::StructOpt;
use tempfile::PersistError;

use crate::command::Command;
use crate::config::{ConfigError, TEdgeConfig, C8Y_CONNECT, TEDGE_HOME_DIR};
use crate::utils::{paths, services};

const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";
const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "bridges";

#[derive(thiserror::Error, Debug)]
pub enum DisconnectError {
    #[error(transparent)]
    Configuration(#[from] ConfigError),

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

    #[error("Couldn't find path to 'sudo'. Update $PATH variable with 'sudo' path. \n{0}")]
    SudoNotFound(#[from] which::Error),

    #[error("Provided endpoint url is not valid, provide valid url. \n{0}")]
    UrlParse(#[from] url::ParseError),

    #[error(transparent)]
    ServicesError(#[from] services::ServicesError),
}

#[derive(StructOpt, Debug)]
pub struct Disconnect {}

impl Command for Disconnect {
    fn to_string(&self) -> String {
        "execute 'tedge disconnect'.".into()
    }

    fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        Ok(self.stop_bridge()?)
    }
}

impl Disconnect {
    fn stop_bridge(&self) -> Result<(), DisconnectError> {
        // Check if bridge is active and stop with code 0 if so.
        println!("Checking if bridge exists.");
        match self.check_bridge_config_exists() {
            Ok(false) => {
                let _ = self.save_c8y_config()?;
                println!("Bridge doesn't exist. Operation successful!");
                return Ok(());
            }
            Ok(true) => {}
            Err(e) => return Err(e),
        }

        // Remove bridge file from ~/.tedge/bridges
        println!("Removing c8y bridge.");
        let _ = self.remove_c8y_bridge_config()?;

        // Restart mosquitto
        println!("Applying changes to mosquitto.");
        let _ = services::mosquitto_restart_daemon()?;

        // set c8y.connect to false
        println!("Saving configuration.");
        let _ = self.save_c8y_config()?;

        println!("Bridge successfully disconnected!");
        Ok(())
    }

    fn check_bridge_config_exists(&self) -> Result<bool, DisconnectError> {
        let path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            C8Y_CONFIG_FILENAME,
        ])?;

        // Using metadata as .exists doesn't fail if no permission.
        match Path::new(&path).metadata() {
            Ok(meta) => Ok(meta.is_file()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    fn remove_c8y_bridge_config(&self) -> Result<(), DisconnectError> {
        let path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            C8Y_CONFIG_FILENAME,
        ])?;
        let _ = std::fs::remove_file(&path).or_else(services::ok_if_not_found)?;

        Ok(())
    }

    fn save_c8y_config(&self) -> Result<(), DisconnectError> {
        let mut config = TEdgeConfig::from_default_config()?;
        TEdgeConfig::set_config_value(&mut config, C8Y_CONNECT, "false".into())?;
        Ok(TEdgeConfig::write_to_default_config(&config)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORRECT_URL: &str = "http://test.com";
    const INCORRECT_URL: &str = "noturl";
    const INCORRECT_PATH: &str = "/path";
}
