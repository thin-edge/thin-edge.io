use crate::command::Command;
use structopt::StructOpt;

mod c8y;

#[derive(StructOpt, Debug)]
pub enum DisconnectCmd {
    /// Remove bridge connection to Cumulocity.
    C8y(c8y::Disconnect),
}

impl DisconnectCmd {
    fn sub_command(&self) -> &dyn Command {
        match self {
            DisconnectCmd::C8y(cmd) => cmd,
        }
    }
}

impl Command for DisconnectCmd {
    fn to_string(&self) -> String {
        self.sub_command().to_string()
    }

    fn run(&self, verbose: u8) -> Result<(), anyhow::Error> {
        self.sub_command().run(verbose)
    }
}
