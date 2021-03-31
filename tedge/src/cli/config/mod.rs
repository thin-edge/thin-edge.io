use crate::command::{BuildCommand, Command};
pub use cli::*;
use tedge_config::*;

mod cli;
mod commands;
pub mod config_keys;

impl BuildCommand for cli::ConfigCmd {
    fn build_command(self, config: TEdgeConfig) -> Result<Box<dyn Command>, TEdgeConfigError> {
        Ok(match self {
            cli::ConfigCmd::Get { key } => commands::GetConfigCommand { key, config }.into_boxed(),
            cli::ConfigCmd::Set { key, value } => commands::SetConfigCommand {
                key,
                value,
                config_manager: TEdgeConfigManager::try_default()?,
            }
            .into_boxed(),
            cli::ConfigCmd::Unset { key } => commands::UnsetConfigCommand {
                key,
                config_manager: TEdgeConfigManager::try_default()?,
            }
            .into_boxed(),
            cli::ConfigCmd::List { is_all, is_doc } => commands::ListConfigCommand {
                is_all,
                is_doc,
                config,
            }
            .into_boxed(),
        })
    }
}
