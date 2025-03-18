use crate::cli::common::CloudArg;
use crate::cli::disconnect::disconnect_bridge::*;
use crate::command::*;
use crate::system_services::service_manager;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;

#[derive(clap::Args, Debug)]
pub struct TEdgeDisconnectBridgeCli {
    #[clap(subcommand)]
    cloud: CloudArg,
}

impl BuildCommand for TEdgeDisconnectBridgeCli {
    fn build_command(
        self,
        _: TEdgeConfig,
        config_location: TEdgeConfigLocation,
    ) -> Result<Box<dyn Command>, crate::ConfigError> {
        let cmd = DisconnectBridgeCommand {
            config_location: config_location.clone(),
            cloud: self.cloud.try_into()?,
            use_mapper: true,
            service_manager: service_manager(&config_location.tedge_config_root_path)?,
        };
        Ok(cmd.into_boxed())
    }
}
