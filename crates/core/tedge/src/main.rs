#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use anyhow::Context;
use clap::Parser;
mod cli;
mod command;
mod error;
use command::BuildCommand;
use command::BuildContext;
use tedge_config::system_services::set_log_level;
use tracing::log::warn;

type ConfigError = crate::error::TEdgeError;

const BROKER_USER: &str = "mosquitto";
const BROKER_GROUP: &str = "mosquitto";

fn main() -> anyhow::Result<()> {
    set_log_level(tracing::Level::WARN);

    let opt = cli::Opt::parse();

    if opt.init {
        warn!("This --init option has been deprecated and will be removed in a future release");
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
