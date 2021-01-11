use structopt::StructOpt;

mod cli;
mod certificate;

fn main() {
    let opt = cli::Opt::from_args();
    println!("{:?}", opt);
}
