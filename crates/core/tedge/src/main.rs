#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

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
    let user_manager = UserManager::new();
    if opt.init {
        println!("Initialize the tedge");
        initialize_tedge()?;
        return Ok(());
    }

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

fn initialize_tedge() -> anyhow::Result<()> {
    create_directory_with_user_group(
        "tedge",
        vec![
            "/etc/tedge",
            "/var/log/tedge",
            "/etc/tedge/mosquitto-conf",
            "/etc/tedge/operations",
            "/etc/tedge/plugins",
        ],
    )?;
    create_directory_with_user_group("mosquitto", vec!["/etc/tedge/device-certs"])?;
    Ok(())
}
