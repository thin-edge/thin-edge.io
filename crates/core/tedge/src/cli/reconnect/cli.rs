use crate::cli::common::Cloud;
use crate::cli::connect::CommonMosquittoConfig;
use crate::command::*;
use tedge_config::system_services::service_manager;

use super::command::ReconnectBridgeCommand;

const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";
const AZURE_CONFIG_FILENAME: &str = "az-bridge.conf";
const AWS_CONFIG_FILENAME: &str = "aws-bridge.conf";

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeReconnectCli {
    /// Remove bridge connection to Cumulocity.
    C8y,
    /// Remove bridge connection to Azure.
    Az,
    /// Remove bridge connection to AWS.
    Aws,
}

impl BuildCommand for TEdgeReconnectCli {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError> {
        let config_location = context.config_location.clone();
        let config_repository = context.config_repository;
        let service_manager = service_manager(&context.config_location.tedge_config_root_path)?;
        let common_mosquitto_config = CommonMosquittoConfig::default();

        let cmd = match self {
            TEdgeReconnectCli::C8y => ReconnectBridgeCommand {
                config_location,
                config_repository,
                service_manager,
                common_mosquitto_config,
                config_file: C8Y_CONFIG_FILENAME.into(),
                cloud: Cloud::C8y,
                use_mapper: true,
                use_agent: true,
            },
            TEdgeReconnectCli::Az => ReconnectBridgeCommand {
                config_location,
                config_repository,
                service_manager,
                common_mosquitto_config,
                config_file: AZURE_CONFIG_FILENAME.into(),
                cloud: Cloud::Azure,
                use_mapper: true,
                use_agent: false,
            },
            TEdgeReconnectCli::Aws => ReconnectBridgeCommand {
                config_location,
                config_repository,
                service_manager,
                common_mosquitto_config,
                config_file: AWS_CONFIG_FILENAME.into(),
                cloud: Cloud::Aws,
                use_mapper: true,
                use_agent: false,
            },
        };
        Ok(cmd.into_boxed())
    }
}
