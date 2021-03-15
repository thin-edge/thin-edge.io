use crate::command::{BuildCommand, Command};
use crate::config::{ConfigError, TEDGE_HOME_DIR};
use crate::utils::{paths, services};
use structopt::StructOpt;

const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "bridges";
#[derive(StructOpt, Debug)]
pub enum TedgeDisconnectBridgeOpt {
    /// Remove bridge connection to Cumulocity.
    C8y,
    Az,
}

impl BuildCommand for TedgeDisconnectBridgeOpt {
    fn build_command(
        self,
        _tedge_config: crate::config::TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::config::ConfigError> {
        let cmd = match self {
            TedgeDisconnectBridgeOpt::C8y => DisconnectBridge {
                cloud_name: String::from("c8y"),
            },
            TedgeDisconnectBridgeOpt::Az => DisconnectBridge {
                cloud_name: String::from("az"),
            },
        };
        Ok(cmd.into_boxed())
    }
}

#[derive(StructOpt, Debug)]
pub struct DisconnectBridge {
    cloud_name: String,
}

impl Command for DisconnectBridge {
    fn description(&self) -> String {
        "execute 'tedge disconnect'.".into()
    }

    fn execute(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        Ok(self.stop_bridge()?)
    }
}

impl DisconnectBridge {
    fn stop_bridge(&self) -> Result<(), DisconnectBridgeError> {
        // Check if bridge exists and stop with code 0 if it doesn't.
        let bridge_conf_path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            &self.get_bridge_config_file_name(),
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

            Err(e) => Err(DisconnectBridgeError::FileOperationFailed(
                e,
                bridge_conf_path,
            )),
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

    fn get_bridge_config_file_name(&self) -> String {
        self.cloud_name.clone() + "-bridge.conf"
    }
}
#[derive(thiserror::Error, Debug)]
pub enum DisconnectBridgeError {
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
