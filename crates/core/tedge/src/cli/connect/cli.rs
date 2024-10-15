use tedge_config::system_services::service_manager;
use tedge_config::ProfileName;

use crate::cli::common::Cloud;
use crate::cli::connect::*;
use crate::command::BuildCommand;
use crate::command::BuildContext;
use crate::command::Command;

#[derive(clap::Subcommand, Debug, Eq, PartialEq)]
pub enum TEdgeConnectOpt {
    /// Create connection to Cumulocity
    ///
    /// The command will create config and start edge relay from the device to c8y instance
    C8y {
        /// Test connection to Cumulocity
        #[clap(long = "test")]
        is_test_connection: bool,

        /// Ignore connection registration and connection check
        #[clap(long = "offline")]
        offline_mode: bool,

        #[clap(long, hide = true)]
        profile: Option<ProfileName>,
    },

    /// Create connection to Azure
    ///
    /// The command will create config and start edge relay from the device to az instance
    Az {
        /// Test connection to Azure
        #[clap(long = "test")]
        is_test_connection: bool,

        /// Ignore connection registration and connection check
        #[clap(long = "offline")]
        offline_mode: bool,

        #[clap(long, hide = true)]
        profile: Option<ProfileName>,
    },

    /// Create connection to AWS
    ///
    /// The command will create config and start edge relay from the device to AWS instance
    Aws {
        /// Test connection to AWS
        #[clap(long = "test")]
        is_test_connection: bool,

        /// Ignore connection registration and connection check
        #[clap(long = "offline")]
        offline_mode: bool,

        #[clap(long, hide = true)]
        profile: Option<ProfileName>,
    },
}

impl BuildCommand for TEdgeConnectOpt {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError> {
        Ok(match self {
            TEdgeConnectOpt::C8y {
                is_test_connection,
                offline_mode,
                profile,
            } => ConnectCommand {
                config_location: context.config_location.clone(),
                config: context.load_config()?,
                cloud: Cloud::C8y,
                is_test_connection,
                offline_mode,
                service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
                profile,
            },
            TEdgeConnectOpt::Az {
                is_test_connection,
                offline_mode,
                profile,
            } => ConnectCommand {
                config_location: context.config_location.clone(),
                config: context.load_config()?,
                cloud: Cloud::Azure,
                is_test_connection,
                offline_mode,
                service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
                profile,
            },
            TEdgeConnectOpt::Aws {
                is_test_connection,
                offline_mode,
                profile,
            } => ConnectCommand {
                config_location: context.config_location.clone(),
                config: context.load_config()?,
                cloud: Cloud::Aws,
                is_test_connection,
                offline_mode,
                service_manager: service_manager(&context.config_location.tedge_config_root_path)?,
                profile,
            },
        }
        .into_boxed())
    }
}
