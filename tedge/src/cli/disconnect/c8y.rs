use structopt::StructOpt;

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

    #[error(transparent)]
    PathsError(#[from] paths::PathsError),

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
        // Check if bridge exists and stop with code 0 if it doesn't.
        println!("Checking if bridge exists.\n");
        let bridge_conf_path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            C8Y_CONFIG_FILENAME,
        ])?;

        match paths::check_path_exists(&bridge_conf_path) {
            Ok(true) => {
                // Remove bridge file from ~/.tedge/bridges
                println!("Removing c8y bridge.\n");
                let _ = std::fs::remove_file(&bridge_conf_path)?;
            }

            Ok(false) => {
                // We need to set c8y.connect to 'false' here as it may have been 'true' before to be in 'actual state'.
                let _ = self.set_connect_and_save_tedge_config()?;
                println!("Bridge doesn't exist. Operation successful!");
                return Ok(());
            }

            Err(e) => return Err(e.into()),
        }

        // Deviation from specification:
        // * Check if mosquitto is running, restart only if was active before, if not don't do anything.
        println!("Applying changes to mosquitto.\n");
        match services::check_mosquitto_is_running() {
            Ok(()) => services::mosquitto_restart_daemon()?,
            Err(e) => match e {
                services::ServicesError::NonZeroReturnCode { code } => (),
                _ => return Err(e.into()),
            },
        }

        // set c8y.connect to false
        println!("Saving configuration.\n");
        let _ = self.set_connect_and_save_tedge_config()?;

        println!("Bridge successfully disconnected!");
        Ok(())
    }

    fn set_connect_and_save_tedge_config(&self) -> Result<(), DisconnectError> {
        let mut config = TEdgeConfig::from_default_config()?;
        TEdgeConfig::set_config_value(&mut config, C8Y_CONNECT, "false".into())?;
        Ok(TEdgeConfig::write_to_default_config(&config)?)
    }
}
