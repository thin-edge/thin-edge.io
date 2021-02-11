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

impl ConnectCmd {
    fn sub_command(&self) -> &dyn Command {
        match self {
            ConnectCmd::C8y(cmd) => cmd,
        }
    }
}

impl Command for ConnectCmd {
    fn to_string(&self) -> String {
        self.sub_command().to_string()
    }

    fn run(&self, verbose: u8) -> Result<(), anyhow::Error> {
        self.sub_command().run(verbose)
    }
}
