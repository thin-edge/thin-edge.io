use structopt::StructOpt;

mod cli;

/**
tedge 0.1.0


USAGE:
    tedge [FLAGS] --config <config> <SUBCOMMAND>

*/

fn main() {
    let _opt = cli::Opt::from_args();
}
