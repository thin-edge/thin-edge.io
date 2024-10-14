use crate::cli::common::Cloud;
use crate::command::*;
use tedge_config::system_services::service_manager;
use tedge_config::ProfileName;
use super::command::ReconnectBridgeCommand;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeReconnectCli {
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

impl BuildCommand for TEdgeReconnectCli {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError> {
        let config_location = context.config_location.clone();
        let config = context.load_config()?;
        let service_manager = service_manager(&context.config_location.tedge_config_root_path)?;

        let cmd = match self {
            TEdgeReconnectCli::C8y { profile } => ReconnectBridgeCommand {
                config_location,
                config,
                service_manager,
                cloud: Cloud::C8y,
                use_mapper: true,
                profile,
            },
            TEdgeReconnectCli::Az { profile} => ReconnectBridgeCommand {
                config_location,
                config,
                service_manager,
                cloud: Cloud::Azure,
                use_mapper: true,
                profile,
            },
            TEdgeReconnectCli::Aws { profile} => ReconnectBridgeCommand {
                config_location,
                config,
                service_manager,
                cloud: Cloud::Aws,
                use_mapper: true,
                profile,
            },
        };
        Ok(cmd.into_boxed())
    }
}
