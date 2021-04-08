#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use anyhow::Context;
use structopt::StructOpt;

mod certificate;
mod cli;
mod command;
mod config;
mod mqtt;
mod services;
mod utils;

use command::{BuildCommand, BuildCommandContext, ExecutionContext};

fn main() -> anyhow::Result<()> {
    let context = ExecutionContext::new();

    let _user_guard = context.user_manager.become_user(utils::users::TEDGE_USER)?;

    let opt = cli::Opt::from_args();

    let config = config::TEdgeConfig::from_default_config()
        .with_context(|| "failed to read the tedge configuration")?;

    let tedge_config_location = if crate::utils::users::UserManager::running_as_root() {
        tedge_config::TEdgeConfigLocation::from_default_system_location()
    } else {
        tedge_config::TEdgeConfigLocation::from_users_home_location(
            crate::utils::paths::home_dir()
                .ok_or(crate::utils::paths::PathsError::HomeDirNotFound)?,
        )
    };
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location);

    let build_context = BuildCommandContext {
        config,
        config_repository,
    };

    let cmd = opt
        .tedge
        .build_command(build_context)
        .with_context(|| "missing configuration parameter")?;

    cmd.execute(&context)
        .with_context(|| format!("failed to {}", cmd.description()))
}
