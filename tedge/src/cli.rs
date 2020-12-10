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
    #[structopt(short, long, parse(from_occurrences))]
    verbose: u8,

    /// Use given config file
    #[structopt(short, long, parse(from_os_str))]
    config: PathBuf,

    #[structopt(subcommand)]
    config_command: Config,
}

#[derive(StructOpt, Debug)]
enum Config {
    /// Configure Thin Edge.
    config {
        #[structopt(subcommand)]
        list: ConfigSubcommands,
    },
}

#[derive(StructOpt, Debug)]
enum ConfigSubcommands {
    /// List all.
    list,

    /// Add a new variable (overwrite the value if the key exists).
    set,

    /// Remove a variable.
    remove,

    /// Get value.
    get,
}
