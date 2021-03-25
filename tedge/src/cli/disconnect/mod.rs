use crate::cli::connect::{
    az::AZURE_CONFIG_FILENAME, c8y::C8Y_CONFIG_FILENAME, TEDGE_BRIDGE_CONF_DIR_PATH,
};
use crate::command::{BuildCommand, Command};
use crate::config::{ConfigError, TEDGE_HOME_DIR};
use crate::services::{
    self, mosquitto::MosquittoService, tedge_mapper::TedgeMapperService, SystemdService,
};
use crate::utils::paths;
use structopt::StructOpt;

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
                mapper: true,
            },
            TedgeDisconnectBridgeOpt::Az => DisconnectBridge {
                config_file: AZURE_CONFIG_FILENAME.into(),
                cloud_name: "Azure".into(),
                mapper: false,
            },
        };
        Ok(cmd.into_boxed())
    }
}

#[derive(Debug)]
pub struct DisconnectBridge {
    config_file: String,
    cloud_name: String,
    mapper: bool,
}

impl Command for DisconnectBridge {
    fn description(&self) -> String {
        format!("remove the bridge to disconnect {} cloud", self.cloud_name)
    }

    fn execute(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        match self.stop_bridge() {
            Ok(()) | Err(DisconnectBridgeError::BridgeFileDoesNotExist) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}

impl DisconnectBridge {
    fn stop_bridge(&self) -> Result<(), DisconnectBridgeError> {
        // If this fails, do not continue with applying changes and stopping/disabling tedge-mapper.
        self.remove_bridge_config_file()?;

        // Ignore failure
        let _ = self.apply_changes_to_mosquitto();

        // Only C8Y changes the status of tedge-mapper
        if self.mapper {
            self.stop_and_disable_tedge_mapper();
        }

        Ok(())
    }

    fn remove_bridge_config_file(&self) -> Result<(), DisconnectBridgeError> {
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
        if MosquittoService.is_active()? {
            MosquittoService.restart()?;
            println!("{} Bridge successfully disconnected!\n", self.cloud_name);
        }
        Ok(())
    }

    fn stop_and_disable_tedge_mapper(&self) {
        let mut failed = false;

        println!("Stopping tedge-mapper service.\n");
        if let Err(err) = TedgeMapperService.stop() {
            println!("Failed to stop tedge-mapper service: {:?}", err);
            failed = true;
        }

        println!("Disabling tedge-mapper service.\n");
        if let Err(err) = TedgeMapperService.disable() {
            println!("Failed to disable tedge-mapper service: {:?}", err);
            failed = true;
        }

        if !failed {
            println!("tedge-mapper service successfully stopped and disabled!\n");
        }
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

    #[error("Bridge file does not exist.")]
    BridgeFileDoesNotExist,
}
