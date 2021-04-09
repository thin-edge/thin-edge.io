#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use anyhow::Context;
use structopt::StructOpt;

fn main() -> anyhow::Result<()> {
    let cli = tedge_cli::TEdgeCli::from_args();

    let tedge_config_location = if crate::utils::users::UserManager::running_as_root() {
        tedge_config::TEdgeConfigLocation::from_default_system_location()
    } else {
        tedge_config::TEdgeConfigLocation::from_users_home_location(
            crate::utils::paths::home_dir()
                .ok_or(crate::utils::paths::PathsError::HomeDirNotFound)?,
        )
    };
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location);

    let cmd: TEdgeCommand = cli.into_command(config_repository /*, ... */).
        .with_context(|| "missing configuration parameter")?;

    let context = ExecutionContext::new();
    let description = cmd.description(); // we have to call it before `execute` as `execute` consumes `self`.

    cmd.execute(&context)
        .with_context(|| format!("failed to {}", cmd.description()))
}
