use crate::cli::common::Cloud;
use crate::cli::disconnect::error::*;
use crate::cli::log::Fancy;
use crate::cli::log::Spinner;
use crate::command::*;
use crate::log::MaybeFancy;
use crate::system_services::*;
use anyhow::Context;
use camino::Utf8PathBuf;
use std::sync::Arc;
use tedge_config::TEdgeConfig;
use which::which;

const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "mosquitto-conf";

#[derive(Debug)]
pub struct DisconnectBridgeCommand {
    pub config_dir: Utf8PathBuf,
    pub cloud: Cloud,
    pub use_mapper: bool,
    pub service_manager: Arc<dyn SystemServiceManager>,
}

#[async_trait::async_trait]
impl Command for DisconnectBridgeCommand {
    fn description(&self) -> String {
        format!("remove the bridge to disconnect {} cloud", self.cloud)
    }

    async fn execute(&self, _: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        self.execute_direct().await
    }
}

impl DisconnectBridgeCommand {
    /// Execute this without needing to pass in a `TEdgeConfig` value
    pub(crate) async fn execute_direct(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        match self.stop_bridge().await {
            Ok(())
            | Err(Fancy {
                err: DisconnectBridgeError::BridgeFileDoesNotExist,
                ..
            }) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    async fn stop_bridge(&self) -> Result<(), Fancy<DisconnectBridgeError>> {
        // If this fails, do not continue with applying changes and stopping/disabling tedge-mapper.
        let is_fatal_error = |err: &DisconnectBridgeError| {
            !matches!(err, DisconnectBridgeError::BridgeFileDoesNotExist)
        };
        let res = Spinner::start_filter_errors("Removing bridge config file", is_fatal_error)
            .finish(self.remove_bridge_config_file().await);
        if res
            .as_ref()
            .err()
            .filter(|e| !is_fatal_error(&e.err))
            .is_some()
        {
            println!(
                "Bridge doesn't exist. Device is already disconnected from {}.",
                self.cloud
            );
            return Ok(());
        } else {
            res?
        }

        if let Err(SystemServiceError::ServiceManagerUnavailable { cmd: _, name }) =
            self.service_manager.check_operational().await
        {
            println!(
                "Service manager '{name}' is not available, skipping stopping/disabling of tedge components.",
            );
            return Ok(());
        }

        // Ignore failure
        let _ = self.apply_changes_to_mosquitto().await;

        // Only C8Y changes the status of tedge-mapper
        if self.use_mapper && which("tedge-mapper").is_ok() {
            let spinner = Spinner::start(format!("Disabling {}", self.cloud.mapper_service()));
            spinner.finish(self.stop_and_disable_mapper().await)?;
        }

        Ok(())
    }

    async fn remove_bridge_config_file(&self) -> Result<(), DisconnectBridgeError> {
        let config_file = self.cloud.bridge_config_filename();
        let bridge_conf_path = self
            .config_dir
            .join(TEDGE_BRIDGE_CONF_DIR_PATH)
            .join(config_file.as_ref());

        let mut result = match tokio::fs::remove_file(&bridge_conf_path).await {
            // If we find the bridge config file we remove it
            // and carry on to see if we need to restart mosquitto.
            Ok(()) => Ok(()),

            // If bridge config file was not found we assume that the bridge doesn't exist,
            // We finish early returning exit code 0.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(DisconnectBridgeError::BridgeFileDoesNotExist)
            }

            Err(e) => Err(e)
                .with_context(|| format!("Failed to delete {bridge_conf_path}"))
                .map_err(|e| e.into()),
        };

        if let Some(path) = self.c8y_mqtt_service_bridge_config_path() {
            let res = match tokio::fs::remove_file(&path).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e)
                    .with_context(|| format!("Failed to delete {path}"))
                    .map_err(|e| e.into()),
            };

            if result.is_ok() {
                result = res;
            }
        }

        result
    }

    pub fn c8y_mqtt_service_bridge_config_path(&self) -> Option<Utf8PathBuf> {
        let bridge_conf_path = self.config_dir.join(TEDGE_BRIDGE_CONF_DIR_PATH);

        match &self.cloud {
            #[cfg(feature = "c8y")]
            Cloud::C8y(None) => Some(bridge_conf_path.join("c8y-mqtt-svc-bridge.conf")),
            #[cfg(feature = "c8y")]
            Cloud::C8y(Some(profile)) => {
                Some(bridge_conf_path.join(format!("c8y-mqtt-svc@{profile}-bridge.conf")))
            }
            _ => None,
        }
    }

    async fn stop_and_disable_mapper(&self) -> Result<(), DisconnectBridgeError> {
        let service = self.cloud.mapper_service();
        self.service_manager.stop_service(service).await?;
        self.service_manager.disable_service(service).await?;
        Ok(())
    }

    // Deviation from specification:
    // Check if mosquitto is running, restart only if it was active before, if not don't do anything.
    async fn apply_changes_to_mosquitto(&self) -> Result<bool, Fancy<DisconnectBridgeError>> {
        restart_service_if_running(&*self.service_manager, SystemService::Mosquitto)
            .await
            .map_err(<_>::into)
    }
}

async fn restart_service_if_running(
    manager: &dyn SystemServiceManager,
    service: SystemService<'_>,
) -> Result<bool, Fancy<SystemServiceError>> {
    if manager.is_service_running(service).await? {
        let spinner = Spinner::start("Restarting mosquitto to apply configuration");
        spinner.finish(manager.restart_service(service).await)?;
        Ok(true)
    } else {
        Ok(false)
    }
}
