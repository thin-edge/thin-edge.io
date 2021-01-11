use structopt::StructOpt;

mod certificate;
mod cli;
mod command;

fn main() {
    let opt = cli::Opt::from_args();
    opt.run().err().map(|err| {
        eprintln!("ERROR: fail to {}", opt.to_string());
        eprintln!("       {}", err);
        std::process::exit(1);
    });
}
