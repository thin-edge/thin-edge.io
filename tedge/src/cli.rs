use super::command::Command;
use crate::config::ConfigCmd;
use structopt::clap;
use structopt::StructOpt;

mod connect;
mod disconnect;

#[derive(StructOpt, Debug)]
#[structopt(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!()
)]
pub struct Opt {
    // The number of occurrences of the `v` flag
    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[structopt(short, parse(from_occurrences))]
    verbose: u8,

    #[structopt(subcommand)]
    tedge_cmd: TEdgeCmd,
}

impl ToString for Opt {
    fn to_string(&self) -> String {
        self.tedge_cmd.to_string()
    }
}

impl Opt {
    pub fn run(&self) -> Result<(), anyhow::Error> {
        self.tedge_cmd.run(self.verbose)
    }
}

#[derive(StructOpt, Debug)]
enum TEdgeCmd {
    /// Create and manage device certificate
    Cert(super::certificate::CertCmd),

    /// Configure Thin Edge.
    Config(ConfigCmd),

    /// Connect to connector provider
    Connect(connect::ConnectCmd),

    /// Publish a message on a topic and subscribe a topic.
    Mqtt(super::mqtt::MqttCmd),
}

impl TEdgeCmd {
    fn sub_command(&self) -> &dyn Command {
        match self {
            TEdgeCmd::Cert(ref cmd) => cmd,
            TEdgeCmd::Config(ref cmd) => cmd,
            TEdgeCmd::Connect(ref cmd) => cmd,
            TEdgeCmd::Mqtt(ref cmd) => cmd,
        }
    }
}

impl Command for TEdgeCmd {
    fn to_string(&self) -> String {
        self.sub_command().to_string()
    }

    fn run(&self, verbose: u8) -> Result<(), anyhow::Error> {
        self.sub_command().run(verbose)
    }
}
