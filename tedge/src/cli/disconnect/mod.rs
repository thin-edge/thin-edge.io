use crate::command::Command;
use structopt::StructOpt;

mod c8y;

#[derive(StructOpt, Debug)]
pub enum DisconnectCmd {
    /// Remove bridge connection to Cumulocity.
    C8y(c8y::Disconnect),
}

impl DisconnectCmd {
    fn sub_command(&self) -> &dyn crate::cli::CliOption {
        match self {
            DisconnectCmd::C8y(cmd) => cmd,
        }
    }
}

impl crate::cli::CliOption for DisconnectCmd {
    fn build_command(&self, config: &crate::config::TEdgeConfig) -> Result<Box<dyn Command>, crate::config::ConfigError> {
        self.sub_command().build_command(config)
    }
}
