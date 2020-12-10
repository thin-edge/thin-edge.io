use std::path::PathBuf;

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

    /// Use given config file
    #[structopt(short, long, parse(from_os_str))]
    config: PathBuf,

    #[structopt(subcommand)]
    subcommand: Subcommand,
}

#[derive(StructOpt, Debug)]
enum Subcommand {
    /// Configure Thin Edge.
    Config {
        #[structopt(subcommand)]
        list: ConfigSubcommand,
    },
}

#[derive(StructOpt, Debug)]
enum ConfigSubcommand {
    /// List all.
    List,

    /// Add a new variable (overwrite the value if the key exists).
    Set,

    /// Remove a variable.
    Remove,

    /// Get value.
    Get,
}
