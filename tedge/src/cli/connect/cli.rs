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
        let cmd = match self {
            TEdgeConnectOpt::C8y => BridgeCommand {
                bridge_config: C8y::c8y_bridge_config(context.config)?,
                check_connection: Box::new(C8y),
            },
            TEdgeConnectOpt::Az => BridgeCommand {
                bridge_config: Azure::azure_bridge_config(context.config)?,
                check_connection: Box::new(Azure),
            },
        };
        Ok(cmd.into_boxed())
    }
}
