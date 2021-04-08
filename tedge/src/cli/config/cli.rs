use crate::cli::config::{commands::*, config_key::*};
use crate::command::{BuildCommand, Command};
use crate::config::*;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub enum ConfigCmd {
    /// Get the value of the provided configuration key
    Get {
        /// Configuration key. Run `tedge config list --doc` for available keys
        key: ConfigKey,
    },

    /// Set or update the provided configuration key with the given value
    Set {
        /// Configuration key. Run `tedge config list --doc` for available keys
        key: ConfigKey,

        /// Configuration value.
        value: String,
    },

    /// Unset the provided configuration key
    Unset {
        /// Configuration key. Run `tedge config list --doc` for available keys
        key: ConfigKey,
    },

    /// Print the configuration keys and their values
    List {
        /// Prints all the configuration keys, even those without a configured value
        #[structopt(long = "all")]
        is_all: bool,

        /// Prints all keys and descriptions with example values
        #[structopt(long = "doc")]
        is_doc: bool,
    },
}

impl BuildCommand for ConfigCmd {
    fn build_command(self, _config: TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        use tedge_config::ConfigRepository;
        let config_repository = tedge_config::TEdgeConfigRepository::new(
            tedge_config::TEdgeConfigLocation::from_default_system_location(),
        );
        let config = config_repository.load()?;

        match self {
            ConfigCmd::Get { key } => Ok(GetConfigCommand {
                config_key: key,
                config,
            }
            .into_boxed()),
            ConfigCmd::Set { key, value } => Ok(SetConfigCommand {
                config_key: key,
                value,
                config,
                config_repository,
            }
            .into_boxed()),
            ConfigCmd::Unset { key } => Ok(UnsetConfigCommand {
                config_key: key,
                config,
                config_repository,
            }
            .into_boxed()),
            ConfigCmd::List { is_all, is_doc } => Ok(ListConfigCommand {
                is_all,
                is_doc,
                config_keys: ConfigKey::list_all(),
                config,
            }
            .into_boxed()),
        }
    }
}
