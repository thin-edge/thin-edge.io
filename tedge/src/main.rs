use structopt::StructOpt;

mod cli;

/**
tedge 0.1.0


USAGE:
    tedge [FLAGS] --config <config> <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v, --verbose    Verbose mode (-v, -vv, -vvv, etc.)

OPTIONS:
    -c, --config <config>    Use given config file

SUBCOMMANDS:
    config    Configure Thin Edge
    help      Prints this message or the help of the given subcommand(s)



tedge-config 0.1.0
Configure Thin Edge

USAGE:
    tedge --config <config> config <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    get       Get value
    help      Prints this message or the help of the given subcommand(s)
    list      List all
    remove    Remove a variable
    set       Add a new variable (overwrite the value if the key exists)

*/

fn main() {
    let opt = cli::Opt::from_args();
}
