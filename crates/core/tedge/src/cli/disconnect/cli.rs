use crate::cli::common::Cloud;
use crate::cli::disconnect::disconnect_bridge::*;
use crate::command::*;
use tedge_config::system_services::service_manager;

use crate::bridge::AWS_CONFIG_FILENAME;
use crate::bridge::AZURE_CONFIG_FILENAME;
use crate::bridge::C8Y_CONFIG_FILENAME;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeDisconnectBridgeCli {
    /// Remove bridge connection to Cumulocity.
    C8y,
    /// Remove bridge connection to Azure.
    Az,
    /// Remove bridge connection to AWS.
    Aws,
}

impl BuildCommand for TEdgeDisconnectBridgeCli {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError> {
        let tedge_config = context.load_config()?;
        let cmd = match self {
            TEdgeDisconnectBridgeCli::C8y => DisconnectBridgeCommand {
                config_location: context.config_location.clone(),
                config_file: C8Y_CONFIG_FILENAME.into(),
                cloud: Cloud::C8y,
                use_mapper: true,
                service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
                built_in_bridge: tedge_config.mqtt.bridge.built_in,
            },
            TEdgeDisconnectBridgeCli::Az => DisconnectBridgeCommand {
                config_location: context.config_location.clone(),
                config_file: AZURE_CONFIG_FILENAME.into(),
                cloud: Cloud::Azure,
                use_mapper: true,
                service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
                built_in_bridge: tedge_config.mqtt.bridge.built_in,
            },
            TEdgeDisconnectBridgeCli::Aws => DisconnectBridgeCommand {
                config_location: context.config_location.clone(),
                config_file: AWS_CONFIG_FILENAME.into(),
                cloud: Cloud::Aws,
                use_mapper: true,
                service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
                built_in_bridge: tedge_config.mqtt.bridge.built_in,
            },
        };
        Ok(cmd.into_boxed())
    }
}
