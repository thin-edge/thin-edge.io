use crate::{
    cli::connect::{az::AZURE_CONFIG_FILENAME, c8y::C8Y_CONFIG_FILENAME},
    command::{BuildCommand, BuildContext, Command},
};

use super::bridge::DisconnectBridge;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub enum TedgeDisconnectBridgeCli {
    /// Remove bridge connection to Cumulocity.
    C8y,
    /// Remove bridge connection to Azure.
    Az,
}

impl BuildCommand for TedgeDisconnectBridgeCli {
    fn build_command(
        self,
        _context: BuildContext,
    ) -> Result<Box<dyn Command>, crate::config::ConfigError> {
        let cmd = match self {
            TedgeDisconnectBridgeCli::C8y => DisconnectBridge {
                config_file: C8Y_CONFIG_FILENAME.into(),
                cloud_name: "Cumulocity".into(),
                use_mapper: true,
            },
            TedgeDisconnectBridgeCli::Az => DisconnectBridge {
                config_file: AZURE_CONFIG_FILENAME.into(),
                cloud_name: "Azure".into(),
                use_mapper: false,
            },
        };
        Ok(cmd.into_boxed())
    }
}
