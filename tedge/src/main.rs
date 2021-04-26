#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use crate::system_command::*;
use crate::system_services::DefaultSystemServiceManagerFactory;
use anyhow::Context;
use std::sync::Arc;
use structopt::StructOpt;

mod cli;
mod command;
mod error;
mod system_command;
mod system_services;
mod utils;

type ConfigError = crate::error::TEdgeError;

use crate::utils::paths;
use command::{BuildCommand, BuildContext, ExecutionContext};

fn running_as_root() -> bool {
    users::get_current_uid() == 0
}

fn main() -> anyhow::Result<()> {
    let system_command_runner = Arc::new(UnixSystemCommandRunner);

    let context = ExecutionContext {
        system_service_manager_factory: Box::new(DefaultSystemServiceManagerFactory::new(
            system_command_runner,
        )),
    };

    let opt = cli::Opt::from_args();

    let tedge_config_location = if running_as_root() {
        tedge_config::TEdgeConfigLocation::from_default_system_location()
    } else {
        tedge_config::TEdgeConfigLocation::from_users_home_location(
            paths::home_dir().ok_or(paths::PathsError::HomeDirNotFound)?,
        )
    };
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());

    let build_context = BuildContext {
        config_repository,
        tedge_config_location,
    };

    let cmd = opt
        .tedge
        .build_command(build_context)
        .with_context(|| "missing configuration parameter")?;

    cmd.execute(&context)
        .with_context(|| format!("failed to {}", cmd.description()))
}
