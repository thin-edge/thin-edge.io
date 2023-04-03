use crate::cli::common::Cloud;
use crate::cli::disconnect::error::*;
use crate::command::*;
use std::sync::Arc;
use tedge_config::system_services::*;
use tedge_config::TEdgeConfigLocation;
use which::which;

const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "mosquitto-conf";

#[derive(Debug)]
pub struct DisconnectBridgeCommand {
    pub config_location: TEdgeConfigLocation,
    pub config_file: String,
    pub cloud: Cloud,
    pub use_mapper: bool,
    pub use_agent: bool,
    pub service_manager: Arc<dyn SystemServiceManager>,
}

impl Command for DisconnectBridgeCommand {
    fn description(&self) -> String {
        format!("remove the bridge to disconnect {} cloud", self.cloud)
    }

    fn execute(&self) -> anyhow::Result<()> {
        match self.stop_bridge() {
            Ok(()) | Err(DisconnectBridgeError::BridgeFileDoesNotExist) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}

impl DisconnectBridgeCommand {
    fn service_manager(&self) -> &dyn SystemServiceManager {
        self.service_manager.as_ref()
    }

    fn stop_bridge(&self) -> Result<(), DisconnectBridgeError> {
        // If this fails, do not continue with applying changes and stopping/disabling tedge-mapper.
        self.remove_bridge_config_file()?;

        if let Err(SystemServiceError::ServiceManagerUnavailable { cmd: _, name }) =
            self.service_manager.check_operational()
        {
            println!(
                "Service manager '{}' is not available, skipping stopping/disabling of tedge components.",
                name
            );
            return Ok(());
        }

        // Ignore failure
        let _ = self.apply_changes_to_mosquitto();

        let mut failed = false;
        // Only C8Y changes the status of tedge-mapper
        if self.use_mapper && which("tedge-mapper").is_ok() {
            failed = self
                .service_manager()
                .stop_and_disable_service(self.cloud.mapper_service(), std::io::stdout());
        }

        if self.use_agent && which("tedge-agent").is_ok() {
            failed = self
                .service_manager()
                .stop_and_disable_service(SystemService::TEdgeSMAgent, std::io::stdout());
        }

        match failed {
            false => Ok(()),
            true => Err(DisconnectBridgeError::ServiceFailed),
        }
    }

    fn remove_bridge_config_file(&self) -> Result<(), DisconnectBridgeError> {
        // Check if bridge exists and stop with code 0 if it doesn't.
        let bridge_conf_path = self
            .config_location
            .tedge_config_root_path
            .join(TEDGE_BRIDGE_CONF_DIR_PATH)
            .join(&self.config_file);

        println!("Removing {} bridge.\n", self.cloud);
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
                bridge_conf_path.into(),
            )),
        }
    }

    // Deviation from specification:
    // Check if mosquitto is running, restart only if it was active before, if not don't do anything.
    fn apply_changes_to_mosquitto(&self) -> Result<(), DisconnectBridgeError> {
        println!("Applying changes to mosquitto.\n");

        if self
            .service_manager()
            .restart_service_if_running(SystemService::Mosquitto)?
        {
            println!("{} Bridge successfully disconnected!\n", self.cloud);
        }
        Ok(())
    }
}
