#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use anyhow::Context;
use clap::Parser;
use tedge_users::UserManager;

mod cli;
mod command;
mod error;
mod system_services;

type ConfigError = crate::error::TEdgeError;

use command::{BuildCommand, BuildContext};

fn main() -> anyhow::Result<()> {
    let user_manager = UserManager::new();

    let _user_guard = user_manager.become_user(tedge_users::TEDGE_USER)?;

    let opt = cli::Opt::parse();

    let tedge_config_location = tedge_config::TEdgeConfigLocation::from_custom_root(opt.config_dir);
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());

    let build_context = BuildContext {
        config_repository,
        config_location: tedge_config_location,
        user_manager,
    };

    let cmd = opt
        .tedge
        .build_command(build_context)
        .with_context(|| "missing configuration parameter")?;

    cmd.execute()
        .with_context(|| format!("failed to {}", cmd.description()))
}
