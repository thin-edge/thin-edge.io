use crate::cli::common::CloudArg;
use crate::cli::connect::*;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::system_services::service_manager;
use tedge_config::TEdgeConfig;

#[derive(clap::Args, Debug, Eq, PartialEq)]
pub struct TEdgeConnectOpt {
    /// Test an existing connection
    #[clap(long = "test", global = true)]
    is_test_connection: bool,

    /// Ignore connection registration and connection check
    #[clap(long = "offline", global = true)]
    offline_mode: bool,

    #[clap(subcommand)]
    cloud: CloudArg,
}

impl BuildCommand for TEdgeConnectOpt {
    fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, crate::ConfigError> {
        let Self {
            is_test_connection,
            offline_mode,
            cloud,
        } = self;
        Ok(Box::new(ConnectCommand {
            service_manager: service_manager(config.root_dir())?,
            cloud: cloud.try_into()?,
            is_test_connection,
            offline_mode,
            is_reconnect: false,
        }))
    }
}
