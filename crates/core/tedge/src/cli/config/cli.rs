use crate::cli::config::commands::*;
use crate::command::*;
use crate::ConfigError;
use tedge_config::new::ReadableKey;
use tedge_config::new::WritableKey;

#[derive(clap::Subcommand, Debug)]
pub enum ConfigCmd {
    /// Get the value of the provided configuration key
    Get {
        /// Configuration key. Run `tedge config list --doc` for available keys
        key: ReadableKey,
    },

    /// Set or update the provided configuration key with the given value
    Set {
        /// Configuration key. Run `tedge config list --doc` for available keys
        key: WritableKey,

        /// Configuration value.
        value: String,
    },

    /// Unset the provided configuration key
    Unset {
        /// Configuration key. Run `tedge config list --doc` for available keys
        key: WritableKey,
    },

    /// Print the configuration keys and their values
    List {
        /// Prints all the configuration keys, even those without a configured value
        #[clap(long = "all")]
        is_all: bool,

        /// Prints all keys and descriptions with example values
        #[clap(long = "doc")]
        is_doc: bool,
    },
}

impl BuildCommand for ConfigCmd {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, ConfigError> {
        let config_repository = context.config_repository;

        match self {
            ConfigCmd::Get { key } => Ok(GetConfigCommand {
                key,
                config: config_repository.load_new()?,
            }
            .into_boxed()),
            ConfigCmd::Set { key, value } => Ok(SetConfigCommand {
                key,
                value,
                config_repository,
            }
            .into_boxed()),
            ConfigCmd::Unset { key } => Ok(UnsetConfigCommand {
                key,
                config_repository,
            }
            .into_boxed()),
            ConfigCmd::List { is_all, is_doc } => Ok(ListConfigCommand {
                is_all,
                is_doc,
                config: config_repository.load_new()?,
            }
            .into_boxed()),
        }
    }
}
