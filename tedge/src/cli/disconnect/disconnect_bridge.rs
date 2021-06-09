use crate::cli::disconnect::error::*;
use crate::command::*;
use crate::services::{
    mosquitto::MosquittoService, tedge_mapper_az::TedgeMapperAzService,
    tedge_mapper_c8y::TedgeMapperC8yService, SystemdService,
};
use crate::system_commands::*;
use std::sync::Arc;
use tedge_config::TEdgeConfigLocation;
use which::which;

const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "mosquitto-conf";

#[derive(Debug)]
pub enum Cloud {
    C8y,
    Azure,
}

impl From<Cloud> for String {
    fn from(val: Cloud) -> Self {
        match val {
            Cloud::C8y => "Cumulocity".into(),
            Cloud::Azure => "Azure".into(),
        }
    }
}

#[derive(Debug)]
pub struct DisconnectBridgeCommand {
    pub system_command_runner: Arc<dyn AbstractSystemCommandRunner>,
    pub config_location: TEdgeConfigLocation,
    pub config_file: String,
    pub cloud: Cloud,
    pub use_mapper: bool,
}

impl Command for DisconnectBridgeCommand {
    fn description(&self) -> String {
        format!("remove the bridge to disconnect {:?} cloud", self.cloud)
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        match self.stop_bridge() {
            Ok(()) | Err(DisconnectBridgeError::BridgeFileDoesNotExist) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}

impl DisconnectBridgeCommand {
    fn stop_bridge(&self) -> Result<(), DisconnectBridgeError> {
        // If this fails, do not continue with applying changes and stopping/disabling tedge-mapper.
        self.remove_bridge_config_file()?;

        // Ignore failure
        let _ = self.apply_changes_to_mosquitto();

        // Only C8Y changes the status of tedge-mapper
        if self.use_mapper && which("tedge_mapper").is_ok() {
            match self.cloud {
                Cloud::Azure => {
                    self.stop_and_disable_tedge_mapper_az();
                }
                Cloud::C8y => {
                    self.stop_and_disable_tedge_mapper_c8y();
                }
            }
        }

        Ok(())
    }

    fn remove_bridge_config_file(&self) -> Result<(), DisconnectBridgeError> {
        // Check if bridge exists and stop with code 0 if it doesn't.
        let bridge_conf_path = self
            .config_location
            .tedge_config_root_path
            .join(TEDGE_BRIDGE_CONF_DIR_PATH)
            .join(&self.config_file);

        println!("Removing {:?} bridge.\n", self.cloud);
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
    fn apply_changes_to_mosquitto(&self) -> Result<(), DisconnectBridgeError> {
        println!("Applying changes to mosquitto.\n");
        if MosquittoService.is_active(self.system_command_runner.as_ref())? {
            MosquittoService.restart(self.system_command_runner.as_ref())?;
            println!("{:?} Bridge successfully disconnected!\n", self.cloud);
        }
        Ok(())
    }

    fn stop_and_disable_tedge_mapper_c8y(&self) {
        let mut failed = false;

        println!("Stopping tedge-mapper service.\n");
        if let Err(err) = TedgeMapperC8yService.stop(self.system_command_runner.as_ref()) {
            println!("Failed to stop tedge-mapper service: {:?}", err);
            failed = true;
        }

        println!("Disabling tedge-mapper service.\n");
        if let Err(err) = TedgeMapperC8yService.disable(self.system_command_runner.as_ref()) {
            println!("Failed to disable tedge-mapper service: {:?}", err);
            failed = true;
        }

        if !failed {
            println!("tedge-mapper service successfully stopped and disabled!\n");
        }
    }

    fn stop_and_disable_tedge_mapper_az(&self) {
        let mut failed = false;

        println!("Stopping tedge-mapper service.\n");
        if let Err(err) = TedgeMapperAzService.stop(self.system_command_runner.as_ref()) {
            println!("Failed to stop tedge-mapper service: {:?}", err);
            failed = true;
        }

        println!("Disabling tedge-mapper service.\n");
        if let Err(err) = TedgeMapperAzService.disable(self.system_command_runner.as_ref()) {
            println!("Failed to disable tedge-mapper service: {:?}", err);
            failed = true;
        }

        if !failed {
            println!("tedge-mapper service successfully stopped and disabled!\n");
        }
    }
}
