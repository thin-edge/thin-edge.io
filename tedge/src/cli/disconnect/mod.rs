use crate::command::{BuildCommand, Command};
use structopt::StructOpt;

mod c8y;

#[derive(StructOpt, Debug)]
pub enum DisconnectCmd {
    /// Remove bridge connection to Cumulocity.
    C8y(c8y::Disconnect),
}

impl BuildCommand for DisconnectCmd {
    fn build_command(
        self,
        config: &crate::config::TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::config::ConfigError> {
        match self {
            DisconnectCmd::C8y(opt) => opt.build_command(config),
        }
    }
}
