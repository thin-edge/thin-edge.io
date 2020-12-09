mod cli;

use crate::cli::Options;

/**
tedge 0.1.0
USAGE:
    tedge [FLAGS] [SUBCOMMAND]
FLAGS:
    -h, --help       Prints help information
    -v               Increases the level of verbosity (the max level is -vvv)
    -V, --version    Prints version information
SUBCOMMANDS:
    certificate    Initialize all Thin Edge cert related configuration only by one command.
    help           Prints this message or the help of the given subcommand(s)
*/

fn main() {
    // Load command line options.
    let options = Options::new();
}
