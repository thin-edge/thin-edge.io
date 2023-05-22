#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use std::path::Path;

use anyhow::Context;
use clap::Parser;
mod cli;
mod command;
mod error;
use tedge_config::system_services::set_log_level;
use tedge_utils::file::create_directory_with_user_group;

type ConfigError = crate::error::TEdgeError;

use command::BuildCommand;
use command::BuildContext;

const BROKER_USER: &str = "mosquitto";
const BROKER_GROUP: &str = "mosquitto";

fn main() -> anyhow::Result<()> {
    set_log_level(tracing::Level::WARN);

    let opt = cli::Opt::parse();

    if opt.init {
        initialize_tedge(&opt.config_dir)
            .with_context(|| "Failed to initialize tedge. You have to run tedge with sudo.")?;
        return Ok(());
    }

    let tedge_config_location = tedge_config::TEdgeConfigLocation::from_custom_root(opt.config_dir);
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());

    let build_context = BuildContext {
        config_repository,
        config_location: tedge_config_location,
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

fn initialize_tedge(config_dir: &Path) -> anyhow::Result<()> {
    create_directory_with_user_group(config_dir, "tedge", "tedge", 0o775)?;
    create_directory_with_user_group("/var/log/tedge", "tedge", "tedge", 0o775)?;
    create_directory_with_user_group(
        format!("{}/mosquitto-conf", config_dir.display()),
        "tedge",
        "tedge",
        0o775,
    )?;
    create_directory_with_user_group(
        format!("{}/operations", config_dir.display()),
        "tedge",
        "tedge",
        0o775,
    )?;
    create_directory_with_user_group(
        format!("{}/plugins", config_dir.display()),
        "tedge",
        "tedge",
        0o775,
    )?;
    create_directory_with_user_group(
        format!("{}/device-certs", config_dir.display()),
        "mosquitto",
        "mosquitto",
        0o775,
    )?;
    Ok(())
}
