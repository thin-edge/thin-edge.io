use super::command::ReconnectBridgeCommand;
use crate::cli::common::CloudArgs;
use crate::command::*;
use tedge_config::system_services::service_manager;

#[derive(clap::Args, Debug)]
pub struct TEdgeReconnectCli {
    #[clap(flatten)]
    cloud: CloudArgs,
}

impl BuildCommand for TEdgeReconnectCli {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError> {
        Ok(ReconnectBridgeCommand {
            config: context.load_config()?,
            service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
            config_location: context.config_location,
            cloud: self.cloud.try_into()?,
            use_mapper: true,
        }
        .into_boxed())
    }
}
