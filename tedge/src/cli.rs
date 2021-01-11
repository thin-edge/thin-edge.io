use super::command::Cmd;
use std::error::Error;
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
enum TEdgeCmd {
    /// Configure Thin Edge.
    Config(ConfigCmd),

    /// Create and manage device certificate
    Cert(super::certificate::CertCmd),
}

#[derive(StructOpt, Debug)]
enum ConfigCmd {
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
        format!("{:?}", self.tedge_cmd)
    }
}

impl Opt {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        self.tedge_cmd.run(self.verbose)
    }
}

impl Cmd for TEdgeCmd {
    fn run(&self, verbose: u8) -> Result<(), Box<dyn Error>> {
        match self {
            TEdgeCmd::Config(ref cmd) => cmd.run(verbose),
            TEdgeCmd::Cert(ref cmd) => cmd.run(verbose),
        }
    }
}

impl Cmd for ConfigCmd {
    fn run(&self, _verbose: u8) -> Result<(), Box<dyn Error>> {
        println!("{:?}", self);
        Ok(())
    }
}
