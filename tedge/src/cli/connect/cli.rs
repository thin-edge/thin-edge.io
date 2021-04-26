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
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError> {
        let tedge_config_location = context.tedge_config_location().clone();
        Ok(match self {
            TEdgeConnectOpt::C8y => ConnectCommand {
                config_repository: context.config_repository,
                cloud: Cloud::C8y,
                common_mosquitto_config: CommonMosquittoConfig::default(),
                tedge_config_location,
            },
            TEdgeConnectOpt::Az => ConnectCommand {
                config_repository: context.config_repository,
                cloud: Cloud::Azure,
                common_mosquitto_config: CommonMosquittoConfig::default(),
                tedge_config_location,
            },
        }
        .into_boxed())
    }
}
