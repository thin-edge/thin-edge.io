use crate::cli::connect::{
    az::AZURE_CONFIG_FILENAME, c8y::C8Y_CONFIG_FILENAME, TEDGE_BRIDGE_CONF_DIR_PATH,
};
use crate::command::{BuildCommand, Command};
use crate::config::{ConfigError, TEDGE_HOME_DIR};
use crate::utils::{paths, services};
use structopt::StructOpt;
use crate::utils::users::UserManager;

//const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "bridges";

#[derive(StructOpt, Debug)]
pub enum TedgeDisconnectBridgeOpt {
    /// Remove bridge connection to Cumulocity.
    C8y,
    /// Remove bridge connection to Azure.
    Az,
}

impl BuildCommand for TedgeDisconnectBridgeOpt {
    fn build_command(
        self,
        _tedge_config: crate::config::TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::config::ConfigError> {
        let cmd = match self {
            TedgeDisconnectBridgeOpt::C8y => DisconnectBridge {
                config_file: C8Y_CONFIG_FILENAME.into(),
                cloud_name: "Cumulocity".into(),
            },
            TedgeDisconnectBridgeOpt::Az => DisconnectBridge {
                config_file: AZURE_CONFIG_FILENAME.into(),
                cloud_name: "Azure".into(),
            },
        };
        Ok(cmd.into_boxed())
    }
}

#[derive(StructOpt, Debug)]
pub struct DisconnectBridge {
    config_file: String,
    cloud_name: String,
}

impl Command for DisconnectBridge {
    fn description(&self) -> String {
        format!("execute 'tedge disconnect {}'", self.cloud_name)
    }

    fn execute(&self, user_manager: UserManager) -> Result<(), anyhow::Error> {
        Ok(self.stop_bridge(&user_manager)?)
    }
}

impl DisconnectBridge {
    fn stop_bridge(&self, user_manager: &UserManager) -> Result<(), DisconnectBridgeError> {
        // Check if bridge exists and stop with code 0 if it doesn't.

        let bridge_conf_path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            &self.config_file,
        ])?;
        println!("Removing {} bridge.\n", self.cloud_name);
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
            services::mosquitto_restart_daemon(user_manager)?;
        }

        println!("{} Bridge successfully disconnected!", self.cloud_name);
        Ok(())
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
