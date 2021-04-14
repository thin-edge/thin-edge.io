use crate::{
    cli::{
        connect::{az::AZURE_CONFIG_FILENAME, c8y::C8Y_CONFIG_FILENAME},
        disconnect::disconnect_bridge::DisconnectBridgeCommand,
    },
    command::{BuildCommand, BuildContext, Command},
};

use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub enum TEdgeDisconnectBridgeCli {
    /// Remove bridge connection to Cumulocity.
    C8y,
    /// Remove bridge connection to Azure.
    Az,
}

impl BuildCommand for TEdgeDisconnectBridgeCli {
    fn build_command(
        self,
        _context: BuildContext,
    ) -> Result<Box<dyn Command>, crate::config::ConfigError> {
        let cmd = match self {
            TEdgeDisconnectBridgeCli::C8y => DisconnectBridgeCommand {
                config_file: C8Y_CONFIG_FILENAME.into(),
                cloud_name: "Cumulocity".into(),
                use_mapper: true,
            },
            TEdgeDisconnectBridgeCli::Az => DisconnectBridgeCommand {
                config_file: AZURE_CONFIG_FILENAME.into(),
                cloud_name: "Azure".into(),
                use_mapper: false,
            },
        };
        Ok(cmd.into_boxed())
    }
}
