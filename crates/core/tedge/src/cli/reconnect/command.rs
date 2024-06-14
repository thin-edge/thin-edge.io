use std::sync::Arc;

use crate::cli::common::Cloud;
use crate::cli::connect::ConnectCommand;
use crate::cli::disconnect::disconnect_bridge::DisconnectBridgeCommand;
use crate::command::Command;
use tedge_config::system_services::SystemServiceManager;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;

pub struct ReconnectBridgeCommand {
    pub config_location: TEdgeConfigLocation,
    pub config: TEdgeConfig,
    pub config_file: String,
    pub cloud: Cloud,
    pub use_mapper: bool,
    pub service_manager: Arc<dyn SystemServiceManager>,
}

impl Command for ReconnectBridgeCommand {
    fn description(&self) -> String {
        format!("reconnect {} cloud", self.cloud)
    }

    /// calls the disconnect command, followed by the connect command
    fn execute(&self) -> anyhow::Result<()> {
        let disconnect_cmd: DisconnectBridgeCommand = self.into();
        disconnect_cmd.execute()?;

        let connect_cmd: ConnectCommand = self.into();
        connect_cmd.execute()?;
        Ok(())
    }
}

impl From<&ReconnectBridgeCommand> for DisconnectBridgeCommand {
    fn from(reconnect_cmd: &ReconnectBridgeCommand) -> Self {
        DisconnectBridgeCommand {
            config_location: reconnect_cmd.config_location.clone(),
            config_file: reconnect_cmd.config_file.clone(),
            cloud: reconnect_cmd.cloud,
            use_mapper: reconnect_cmd.use_mapper,
            service_manager: reconnect_cmd.service_manager.clone(),
        }
    }
}

impl From<&ReconnectBridgeCommand> for ConnectCommand {
    fn from(reconnect_cmd: &ReconnectBridgeCommand) -> Self {
        ConnectCommand {
            config_location: reconnect_cmd.config_location.clone(),
            config: reconnect_cmd.config.clone(),
            cloud: reconnect_cmd.cloud,
            is_test_connection: false,
            offline_mode: false,
            service_manager: reconnect_cmd.service_manager.clone(),
        }
    }
}
