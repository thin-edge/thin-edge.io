#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use anyhow::Context;
use clap::Parser;
use tedge::command::BuildCommand;
use tedge::command::BuildContext;
use tedge_config::system_services::set_log_level;
use tracing::log::warn;

fn main() -> anyhow::Result<()> {
    set_log_level(tracing::Level::WARN);

    let opt = tedge::cli::Opt::parse();

    if opt.init {
        warn!("This --init option has been deprecated and will be removed in a future release. Use the `tedge init` command instead");
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
