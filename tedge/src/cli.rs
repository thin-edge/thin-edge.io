use super::command::Command;
use structopt::clap;
use structopt::StructOpt;

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

#[derive(StructOpt, Debug)]
pub enum TEdgeCmd {
    /// Configure Thin Edge.
    Config(ConfigCmd),

    /// Create and manage device certificate
    Cert(super::certificate::CertCmd),
  
    /// Publish a message on a topic and subscribe a topic.
    Mqtt(super::mqtt::MqttCmd),
}

#[derive(StructOpt, Debug)]
pub enum ConfigCmd {
    /// List all.
    List,

    /// Add new value (overwrite the value if the key exists).
    Set { key: String, value: String },

    /// Remove value.
    Unset { key: String },

    /// Get value.
    Get { key: String },
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

impl TEdgeCmd {
    fn sub_command(&self) -> &dyn Command {
        match self {
            TEdgeCmd::Config(ref cmd) => cmd,
            TEdgeCmd::Cert(ref cmd) => cmd,
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

impl Command for ConfigCmd {
    fn to_string(&self) -> String {
        format!("{:?}", self)
    }

    fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        unimplemented!("{:?}", self);
    }
}
