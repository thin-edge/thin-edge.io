use structopt::StructOpt;

use crate::command::{BuildCommand, Command};
use crate::config::{ConfigError, TEdgeConfig, TEDGE_HOME_DIR};
use crate::utils::{paths, services};

const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";
const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "bridges";

#[derive(thiserror::Error, Debug)]
pub enum DisconnectError {
    #[error(transparent)]
    Configuration(#[from] ConfigError),

    #[error("File operation error. Check permissions for {1}.")]
    FileOperationFailed(#[source] std::io::Error, String),

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    PathsError(#[from] paths::PathsError),

    #[error(transparent)]
    ServicesError(#[from] services::ServicesError),
}

#[derive(StructOpt, Debug)]
pub struct Disconnect {}

impl BuildCommand for Disconnect {
    fn build_command(self, _config: TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        // Temporary implementation
        // - should return a specific command, not self.
        // - see certificate.rs for an example
        Ok(self.into_boxed())
    }
}

impl Command for Disconnect {
    fn description(&self) -> String {
        "execute 'tedge disconnect'.".into()
    }

    fn execute(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        Ok(self.stop_bridge()?)
    }
}

impl Disconnect {
    fn stop_bridge(&self) -> Result<(), DisconnectError> {
        // Check if bridge exists and stop with code 0 if it doesn't.
        let bridge_conf_path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            C8Y_CONFIG_FILENAME,
        ])?;

        println!("Removing c8y bridge.\n");
        match std::fs::remove_file(&bridge_conf_path) {
            // If we find the bridge config file we remove it
            // and carry on to see if we need to restart mosquitto.
            Ok(()) => Ok(()),

            // If bridge config file was not found we assume that the bridge doesn't exist,
            // We finish early returning exit code 0.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                println!("Bridge doesn't exist. Operation finished!");
                return Ok(());
            }

            Err(e) => Err(DisconnectError::FileOperationFailed(e, bridge_conf_path)),
        }?;

        // Deviation from specification:
        // * Check if mosquitto is running, restart only if it was active before, if not don't do anything.
        println!("Applying changes to mosquitto.\n");
        if services::check_mosquitto_is_running()? {
            services::mosquitto_restart_daemon()?;
        }

        println!("Bridge successfully disconnected!");
        Ok(())
    }
}
