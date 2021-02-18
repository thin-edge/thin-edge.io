use crate::command::Command;
use structopt::StructOpt;

mod c8y;

#[derive(StructOpt, Debug)]
pub enum ConnectCmd {
    /// Create connection to Cumulocity
    ///
    /// The command will create config and start edge relay from the device to c8y instance
    C8y(c8y::Connect),
}

impl crate::cli::BuildCommand for ConnectCmd {
    fn build_command(
        self,
        config: &crate::config::TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::config::ConfigError> {
        match self {
            ConnectCmd::C8y(opt) => opt.build_command(config),
        }
    }
}
