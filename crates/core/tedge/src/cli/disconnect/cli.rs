use crate::cli::common::CloudArg;
use crate::cli::disconnect::disconnect_bridge::*;
use crate::command::*;
use tedge_config::system_services::service_manager;

#[derive(clap::Args, Debug)]
pub struct TEdgeDisconnectBridgeCli {
    #[clap(subcommand)]
    cloud: CloudArg,
}

impl BuildCommand for TEdgeDisconnectBridgeCli {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError> {
        let cmd = DisconnectBridgeCommand {
            config_location: context.config_location.clone(),
            cloud: self.cloud.try_into()?,
            use_mapper: true,
            service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
        };
        Ok(cmd.into_boxed())
    }
}
