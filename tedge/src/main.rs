#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use anyhow::Context;
use structopt::StructOpt;

mod cli;
mod command;
mod error;
mod services;
mod utils;

type ConfigError = crate::error::TEdgeError;

use command::{BuildCommand, BuildContext, ExecutionContext};

fn main() -> anyhow::Result<()> {
    let context = ExecutionContext::new();

    let _user_guard = context.user_manager.become_user(tedge_users::TEDGE_USER)?;

    let opt = cli::Opt::from_args();

    let tedge_config_location = if tedge_users::UserManager::running_as_root() {
        tedge_config::TEdgeConfigLocation::from_default_system_location()
    } else {
        tedge_config::TEdgeConfigLocation::from_users_home_location(
            crate::utils::paths::home_dir()
                .ok_or(crate::utils::paths::PathsError::HomeDirNotFound)?,
        )
    };
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());

    let build_context = BuildContext {
        config_repository,
        config_location: tedge_config_location,
    };

    let cmd = opt
        .tedge
        .build_command(build_context)
        .with_context(|| "missing configuration parameter")?;

    cmd.execute(&context)
        .with_context(|| format!("failed to {}", cmd.description()))
}
