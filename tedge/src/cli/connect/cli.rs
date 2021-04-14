use crate::cli::connect::*;
use crate::command::{BuildCommand, BuildContext, Command};
use structopt::StructOpt;

#[derive(StructOpt, Debug, PartialEq)]
pub enum TEdgeConnectOpt {
    /// Create connection to Cumulocity
    ///
    /// The command will create config and start edge relay from the device to c8y instance
    C8y,

    /// Create connection to Azure
    ///
    /// The command will create config and start edge relay from the device to az instance
    Az,
}

impl BuildCommand for TEdgeConnectOpt {
    fn build_command(
        self,
        context: BuildContext,
    ) -> Result<Box<dyn Command>, crate::config::ConfigError> {
        Ok(match self {
            TEdgeConnectOpt::C8y => ConnectC8yCommand {
                config_repository: context.config_repository,
            }
            .into_boxed(),
            TEdgeConnectOpt::Az => ConnectAzureCommand {
                config_repository: context.config_repository,
            }
            .into_boxed(),
        })
    }
}
