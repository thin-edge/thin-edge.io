#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use anyhow::Context;
use structopt::StructOpt;

mod certificate;
mod cli;
mod command;
mod config;
mod mqtt;
mod utils;

use crate::cli::BuildCommand;

fn main() -> anyhow::Result<()> {
    let opt = cli::Opt::from_args();
    let config = config::TEdgeConfig::from_default_config()?;
    let cmd = opt.tedge_opt.build_command(&config)?;
    cmd.run(opt.verbose)
        .with_context(|| format!("failed to {}", cmd.description()))
}
