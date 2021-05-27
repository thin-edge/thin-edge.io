use crate::cli::disconnect::disconnect_bridge::*;
use crate::command::*;
use structopt::StructOpt;

const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";
const AZURE_CONFIG_FILENAME: &str = "az-bridge.conf";

#[derive(StructOpt, Debug)]
pub enum TEdgeDisconnectBridgeCli {
    /// Remove bridge connection to Cumulocity.
    C8y,
    /// Remove bridge connection to Azure.
    Az,
}

impl BuildCommand for TEdgeDisconnectBridgeCli {
    fn build_command(self, _context: BuildContext) -> Result<Box<dyn Command>, crate::ConfigError> {
        let cmd = match self {
            TEdgeDisconnectBridgeCli::C8y => DisconnectBridgeCommand {
                config_file: C8Y_CONFIG_FILENAME.into(),
                cloud_name: Cloud::C8y,
                use_mapper: true,
            },
            TEdgeDisconnectBridgeCli::Az => DisconnectBridgeCommand {
                config_file: AZURE_CONFIG_FILENAME.into(),
                cloud_name: Cloud::Azure,
                use_mapper: true,
            },
        };
        Ok(cmd.into_boxed())
    }
}
