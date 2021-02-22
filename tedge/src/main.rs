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

use command::BuildCommand;

fn main() -> anyhow::Result<()> {
    let opt = cli::Opt::from_args();

    let config = config::TEdgeConfig::from_default_config()
        .with_context(|| "failed to read the tedge configuration")?;

    let cmd = opt
        .tedge
        .build_command(&config)
        .with_context(|| "missing configuration parameter")?;

    cmd.execute(opt.verbose)
        .with_context(|| format!("failed to {}", cmd.description()))
}
