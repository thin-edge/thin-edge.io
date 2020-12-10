use structopt::clap;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!()
)]
pub struct Opt {
    // The number of occurrences of the `v/verbose` flag
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
}

#[derive(StructOpt, Debug)]
enum ConfigCmd {
    /// List all.
    List,

    /// Add new value (overwrite the value if the key exists).
    Set { key: String, value: String },

    /// Remove value.
    Remove { key: String },

    /// Get value.
    Get { key: String },
}
