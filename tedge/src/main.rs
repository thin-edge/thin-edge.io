#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use anyhow::Context;
use structopt::StructOpt;

mod certificate;
mod cli;
mod command;
mod config;
mod mqtt;

fn main() -> anyhow::Result<()> {
    let opt = cli::Opt::from_args();
    opt.run()
        .with_context(|| format!("failed to {}", opt.to_string()))
}
