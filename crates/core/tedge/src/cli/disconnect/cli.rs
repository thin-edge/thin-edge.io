use crate::cli::common::Cloud;
use crate::cli::disconnect::disconnect_bridge::*;
use crate::command::*;
use tedge_config::system_services::service_manager;

const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";
const AZURE_CONFIG_FILENAME: &str = "az-bridge.conf";
const AWS_CONFIG_FILENAME: &str = "aws-bridge.conf";

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
        let cmd = match self {
            TEdgeDisconnectBridgeCli::C8y => DisconnectBridgeCommand {
                config_location: context.config_location.clone(),
                config_file: C8Y_CONFIG_FILENAME.into(),
                cloud: Cloud::C8y,
                use_mapper: true,
                use_agent: true,
                service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
            },
            TEdgeDisconnectBridgeCli::Az => DisconnectBridgeCommand {
                config_location: context.config_location.clone(),
                config_file: AZURE_CONFIG_FILENAME.into(),
                cloud: Cloud::Azure,
                use_mapper: true,
                use_agent: false,
                service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
            },
            TEdgeDisconnectBridgeCli::Aws => DisconnectBridgeCommand {
                config_location: context.config_location.clone(),
                config_file: AWS_CONFIG_FILENAME.into(),
                cloud: Cloud::Aws,
                use_mapper: true,
                use_agent: false,
                service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
            },
        };
        Ok(cmd.into_boxed())
    }
}
