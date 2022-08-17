use crate::cli::connect::*;
use crate::command::{BuildCommand, BuildContext, Command};
use crate::system_services::service_manager;

#[derive(clap::Subcommand, Debug, PartialEq)]
pub enum TEdgeConnectOpt {
    /// Create connection to Cumulocity
    ///
    /// The command will create config and start edge relay from the device to c8y instance
    C8y {
        /// Test connection to Cumulocity
        #[clap(long = "test")]
        is_test_connection: bool,
    },

    /// Create connection to Azure
    ///
    /// The command will create config and start edge relay from the device to az instance
    Az {
        /// Test connection to Azure
        #[clap(long = "test")]
        is_test_connection: bool,
    },

    /// Create connection to AWS
    ///
    /// The command will create config and start edge relay from the device to aws instance
    Aws {
        /// Test connection to AWS
        #[clap(long = "test")]
        is_test_connection: bool,
    },
}

impl BuildCommand for TEdgeConnectOpt {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError> {
        Ok(match self {
            TEdgeConnectOpt::C8y { is_test_connection } => ConnectCommand {
                config_location: context.config_location.clone(),
                config_repository: context.config_repository,
                cloud: Cloud::C8y,
                common_mosquitto_config: CommonMosquittoConfig::default(),
                is_test_connection,
                service_manager: service_manager(context.config_location.tedge_config_root_path)?,
            },
            TEdgeConnectOpt::Az { is_test_connection } => ConnectCommand {
                config_location: context.config_location.clone(),
                config_repository: context.config_repository,
                cloud: Cloud::Azure,
                common_mosquitto_config: CommonMosquittoConfig::default(),
                is_test_connection,
                service_manager: service_manager(context.config_location.tedge_config_root_path)?,
            },
            TEdgeConnectOpt::Aws { is_test_connection } => ConnectCommand {
                config_location: context.config_location.clone(),
                config_repository: context.config_repository,
                cloud: Cloud::Aws,
                common_mosquitto_config: CommonMosquittoConfig::default(),
                is_test_connection,
                service_manager: service_manager(context.config_location.tedge_config_root_path)?,
            },
        }
        .into_boxed())
    }
}
