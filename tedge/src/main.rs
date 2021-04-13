#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use anyhow::Context;
use structopt::StructOpt;

mod certificate;
mod cli;
mod command;
mod config;
mod services;
mod utils;

use command::BuildCommand;
use command::ExecutionContext;

fn main() -> anyhow::Result<()> {
    let context = ExecutionContext::new();
    let _user_guard = context.user_manager.become_user(utils::users::TEDGE_USER)?;

    let opt = cli::Opt::from_args();

    let config = config::TEdgeConfig::from_default_config()
        .with_context(|| "failed to read the tedge configuration")?;

    let cmd = opt
        .tedge
        .build_command(config)
        .with_context(|| "missing configuration parameter")?;

    cmd.execute(&context)
        .with_context(|| format!("failed to {}", cmd.description()))
}
