use crate::cli::common::CloudArg;
use crate::cli::disconnect::disconnect_bridge::*;
use crate::command::*;
use crate::system_services::service_manager;
use tedge_config::TEdgeConfig;

#[derive(clap::Args, Debug)]
pub struct TEdgeDisconnectBridgeCli {
    #[clap(subcommand)]
    cloud: CloudArg,
}

impl BuildCommand for TEdgeDisconnectBridgeCli {
    fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, crate::ConfigError> {
        let cmd = DisconnectBridgeCommand {
            service_manager: service_manager(config.root_dir())?,
            config_dir: config.root_dir().to_path_buf(),
            cloud: self.cloud.try_into()?,
            use_mapper: true,
        };
        Ok(cmd.into_boxed())
    }
}
