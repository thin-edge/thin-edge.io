use crate::cli::common::Cloud;
use crate::cli::disconnect::disconnect_bridge::*;
use crate::command::*;
use tedge_config::system_services::service_manager;
use tedge_config::ProfileName;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeDisconnectBridgeCli {
    /// Remove bridge connection to Cumulocity.
    C8y {
        #[clap(long)]
        profile: Option<ProfileName>,
    },
    /// Remove bridge connection to Azure.
    Az {
        #[clap(long)]
        profile: Option<ProfileName>,
    },
    /// Remove bridge connection to AWS.
    Aws {
        #[clap(long)]
        profile: Option<ProfileName>,
    },
}

impl BuildCommand for TEdgeDisconnectBridgeCli {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError> {
        let cmd = match self {
            TEdgeDisconnectBridgeCli::C8y { profile } => DisconnectBridgeCommand {
                config_location: context.config_location.clone(),
                profile,
                cloud: Cloud::C8y,
                use_mapper: true,
                service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
            },
            TEdgeDisconnectBridgeCli::Az { profile } => DisconnectBridgeCommand {
                config_location: context.config_location.clone(),
                profile,
                cloud: Cloud::Azure,
                use_mapper: true,
                service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
            },
            TEdgeDisconnectBridgeCli::Aws { profile } => DisconnectBridgeCommand {
                config_location: context.config_location.clone(),
                profile,
                cloud: Cloud::Aws,
                use_mapper: true,
                service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
            },
        };
        Ok(cmd.into_boxed())
    }
}
