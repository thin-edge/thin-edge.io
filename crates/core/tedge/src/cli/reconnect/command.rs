use camino::Utf8PathBuf;
use tedge_config::TEdgeConfig;

use crate::cli::common::Cloud;
use crate::cli::connect::ConnectCommand;
use crate::cli::disconnect::disconnect_bridge::DisconnectBridgeCommand;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::system_services::SystemServiceManager;
use std::sync::Arc;

pub struct ReconnectBridgeCommand {
    pub config_dir: Utf8PathBuf,
    pub cloud: Cloud,
    pub use_mapper: bool,
    pub service_manager: Arc<dyn SystemServiceManager>,
}

#[async_trait::async_trait]
impl Command for ReconnectBridgeCommand {
    fn description(&self) -> String {
        format!("reconnect {} cloud", self.cloud)
    }

    /// calls the disconnect command, followed by the connect command
    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        eprintln!("Disconnecting from {}", self.cloud);
        let disconnect_cmd: DisconnectBridgeCommand = self.into();
        disconnect_cmd.execute_direct().await?;

        let connect_cmd: ConnectCommand = self.into();
        connect_cmd.execute(config).await?;
        Ok(())
    }
}

impl From<&ReconnectBridgeCommand> for DisconnectBridgeCommand {
    fn from(reconnect_cmd: &ReconnectBridgeCommand) -> Self {
        DisconnectBridgeCommand {
            config_dir: reconnect_cmd.config_dir.clone(),
            cloud: reconnect_cmd.cloud.clone(),
            use_mapper: reconnect_cmd.use_mapper,
            service_manager: reconnect_cmd.service_manager.clone(),
        }
    }
}

impl From<&ReconnectBridgeCommand> for ConnectCommand {
    fn from(reconnect_cmd: &ReconnectBridgeCommand) -> Self {
        ConnectCommand {
            cloud: reconnect_cmd.cloud.clone(),
            is_test_connection: false,
            offline_mode: false,
            service_manager: reconnect_cmd.service_manager.clone(),
            is_reconnect: true,
        }
    }
}
