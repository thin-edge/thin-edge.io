use anyhow::Context;
use structopt::StructOpt;

mod certificate;
mod cli;
mod command;
mod mqtt;

fn main() -> anyhow::Result<()> {
    let opt = cli::Opt::from_args();
    opt.run()
        .with_context(|| format!("fail to {}", opt.to_string()))
}
