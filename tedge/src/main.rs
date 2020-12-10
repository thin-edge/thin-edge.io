use structopt::StructOpt;

mod cli;

fn main() {
    let _opt = cli::Opt::from_args();
    println!("{:?}", _opt);
}
