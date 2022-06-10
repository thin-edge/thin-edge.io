#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use std::path::Path;

use anyhow::Context;
use clap::Parser;
use tedge_users::UserManager;
mod cli;
mod command;
mod error;
mod system_services;
use tedge_utils::file::create_directory_with_user_group;

type ConfigError = crate::error::TEdgeError;

use command::{BuildCommand, BuildContext};

fn main() -> anyhow::Result<()> {
    let opt = cli::Opt::parse();

    if opt.init {
        initialize_tedge(&opt.config_dir)?;
        return Ok(());
    }

    let user_manager = UserManager::new();
    let tedge_config_location = tedge_config::TEdgeConfigLocation::from_custom_root(opt.config_dir);
    let _user_guard = user_manager.become_user(tedge_users::TEDGE_USER)?;
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());

    let build_context = BuildContext {
        config_repository,
        config_location: tedge_config_location,
        user_manager,
    };

    if let Some(tedge_opt) = opt.tedge {
        let cmd = tedge_opt
            .build_command(build_context)
            .with_context(|| "missing configuration parameter")?;

        cmd.execute()
            .with_context(|| format!("failed to {}", cmd.description()))
    } else {
        Ok(())
    }
}

fn initialize_tedge(cfg_dir: &Path) -> anyhow::Result<()> {
    let config_dir = cfg_dir.display().to_string();
    create_directory_with_user_group(&config_dir, "tedge", "tedge", 0o775)?;
    create_directory_with_user_group("/var/log/tedge", "tedge", "tedge", 0o775)?;
    create_directory_with_user_group(
        &format!("{config_dir}/mosquitto-conf"),
        "tedge",
        "tedge",
        0o775,
    )?;
    create_directory_with_user_group(&format!("{config_dir}/operations"), "tedge", "tedge", 0o775)?;
    create_directory_with_user_group(&format!("{config_dir}/plugins"), "tedge", "tedge", 0o775)?;
    create_directory_with_user_group(
        &format!("{config_dir}/device-certs"),
        "mosquitto",
        "mosquitto",
        0o775,
    )?;
    Ok(())
}
