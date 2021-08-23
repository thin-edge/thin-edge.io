use crate::cli::disconnect::error::*;
use crate::command::*;
use crate::system_services::*;
use std::sync::Arc;
use tedge_config::TEdgeConfigLocation;
use which::which;

const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "mosquitto-conf";

#[derive(Copy, Clone, Debug)]
pub enum Cloud {
    C8y,
    Azure,
}

impl Cloud {
    fn dependent_mapper_service(&self) -> SystemService {
        match self {
            Cloud::Azure => SystemService::TEdgeMapperAz,
            Cloud::C8y => SystemService::TEdgeMapperC8y,
        }
    }
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
    pub config_location: TEdgeConfigLocation,
    pub config_file: String,
    pub cloud: Cloud,
    pub use_mapper: bool,
    pub service_manager: Arc<dyn SystemServiceManager>,
}

impl Command for DisconnectBridgeCommand {
    fn description(&self) -> String {
        format!("remove the bridge to disconnect {:?} cloud", self.cloud)
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

        // Ignore failure
        let _ = self.apply_changes_to_mosquitto();

        // Only C8Y changes the status of tedge-mapper
        if self.use_mapper && which("tedge_mapper").is_ok() {
            self.service_manager()
                .stop_and_disable_service(self.cloud.dependent_mapper_service(), std::io::stdout());
        }
        match self.cloud {
            Cloud::C8y => {
                if which("tedge_agent").is_ok() && which("tedge_mapper").is_ok() {
                    self.service_manager().stop_and_disable_service(
                        SystemService::TEdgeSMMapperC8Y,
                        std::io::stdout(),
                    );
                    self.service_manager()
                        .stop_and_disable_service(SystemService::TEdgeSMAgent, std::io::stdout());
                }
            }
            _ => todo!(),
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

        if self
            .service_manager()
            .restart_service_if_running(SystemService::Mosquitto)?
        {
            println!("{:?} Bridge successfully disconnected!\n", self.cloud);
        }
        Ok(())
    }
}
