use crate::command::Command;
use structopt::StructOpt;

mod c8y;

#[derive(StructOpt, Debug)]
pub enum DisconnectCmd {
    /// Remove bridge connection to Cumulocity.
    C8y(c8y::Disconnect),
}

impl crate::cli::BuildCommand for DisconnectCmd {
    fn into_command(
        self,
        config: &crate::config::TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::config::ConfigError> {
        match self {
            DisconnectCmd::C8y(opt) => opt.into_command(config),
        }
    }
}
