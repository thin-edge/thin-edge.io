use crate::cli::common::Cloud;
use crate::cli::connect::*;
use crate::command::BuildCommand;
use crate::command::BuildContext;
use crate::command::Command;
use tedge_config::system_services::service_manager;

#[derive(clap::Args, Debug, Eq, PartialEq)]
pub struct TEdgeConnectOpt {
    /// The cloud you wish to connect to, e.g. `c8y`, `az`, or `aws`
    cloud: Cloud,

    /// Test an existing connection
    #[clap(long = "test")]
    is_test_connection: bool,

    /// Ignore connection registration and connection check
    #[clap(long = "offline")]
    offline_mode: bool,
}

impl BuildCommand for TEdgeConnectOpt {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError> {
        let Self {
            is_test_connection,
            offline_mode,
            cloud,
        } = self;
        Ok(Box::new(ConnectCommand {
            config_location: context.config_location.clone(),
            config: context.load_config()?,
            cloud,
            is_test_connection,
            offline_mode,
            service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
            is_reconnect: false,
        }))
    }
}
