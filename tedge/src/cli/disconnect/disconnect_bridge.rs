use crate::cli::connect::*;
use crate::cli::disconnect::error::*;
use crate::command::*;
use crate::services::{
    mosquitto::MosquittoService, tedge_mapper::TedgeMapperService, SystemdService,
};
use crate::utils::{paths, users::*};

use which::which;

#[derive(Debug)]
pub struct DisconnectBridgeCommand {
    pub config_file: String,
    pub cloud_name: String,
    pub use_mapper: bool,
}

impl Command for DisconnectBridgeCommand {
    fn description(&self) -> String {
        format!("remove the bridge to disconnect {} cloud", self.cloud_name)
    }

    fn execute(&self, context: &ExecutionContext) -> Result<(), anyhow::Error> {
        match self.stop_bridge(&context.user_manager) {
            Ok(()) | Err(DisconnectBridgeError::BridgeFileDoesNotExist) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}

impl DisconnectBridgeCommand {
    fn stop_bridge(&self, user_manager: &UserManager) -> Result<(), DisconnectBridgeError> {
        // If this fails, do not continue with applying changes and stopping/disabling tedge-mapper.
        self.remove_bridge_config_file()?;

        // Ignore failure
        let _ = self.apply_changes_to_mosquitto(user_manager);

        // Only C8Y changes the status of tedge-mapper
        if self.use_mapper && which("tedge_mapper").is_ok() {
            self.stop_and_disable_tedge_mapper(user_manager);
        }

        Ok(())
    }

    fn remove_bridge_config_file(&self) -> Result<(), DisconnectBridgeError> {
        // Check if bridge exists and stop with code 0 if it doesn't.
        let bridge_conf_path =
            paths::build_path_for_sudo_or_user(&[TEDGE_BRIDGE_CONF_DIR_PATH, &self.config_file])?;

        println!("Removing {} bridge.\n", self.cloud_name);
        match std::fs::remove_file(&bridge_conf_path) {
            // If we find the bridge config file we remove it
            // and carry on to see if we need to restart mosquitto.
            Ok(()) => Ok(()),

            // If bridge config file was not found we assume that the bridge doesn't exist,
            // We finish early returning exit code 0.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                println!("Bridge doesn't exist. Operation finished!");
                Err(DisconnectBridgeError::BridgeFileDoesNotExist)
            }

            Err(e) => Err(DisconnectBridgeError::FileOperationFailed(
                e,
                bridge_conf_path,
            )),
        }
    }

    // Deviation from specification:
    // Check if mosquitto is running, restart only if it was active before, if not don't do anything.
    fn apply_changes_to_mosquitto(
        &self,
        user_manager: &UserManager,
    ) -> Result<(), DisconnectBridgeError> {
        println!("Applying changes to mosquitto.\n");
        if MosquittoService.is_active()? {
            MosquittoService.restart(user_manager)?;
            println!("{} Bridge successfully disconnected!\n", self.cloud_name);
        }
        Ok(())
    }

    fn stop_and_disable_tedge_mapper(&self, user_manager: &UserManager) {
        let _root_guard = user_manager.become_user(ROOT_USER);
        let mut failed = false;

        println!("Stopping tedge-mapper service.\n");
        if let Err(err) = TedgeMapperService.stop(user_manager) {
            println!("Failed to stop tedge-mapper service: {:?}", err);
            failed = true;
        }

        println!("Disabling tedge-mapper service.\n");
        if let Err(err) = TedgeMapperService.disable(user_manager) {
            println!("Failed to disable tedge-mapper service: {:?}", err);
            failed = true;
        }

        if !failed {
            println!("tedge-mapper service successfully stopped and disabled!\n");
        }
    }
}
