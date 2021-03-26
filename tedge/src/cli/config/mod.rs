use crate::command::{BuildCommand, Command, ExecutionContext};
use crate::{config::*, settings::*};
use structopt::StructOpt;

mod config_keys;
pub use config_keys::*;

#[derive(StructOpt, Debug)]
pub enum ConfigCmd {
    /// Get the value of the provided configuration key
    Get {
        /// Configuration key.
        #[structopt(help = HELP_VALID_READABLE_KEYS)]
        key: ReadableConfigKey,
    },

    /// Set or update the provided configuration key with the given value
    Set {
        /// Configuration key.
        #[structopt(help = HELP_VALID_WRITABLE_KEYS)]
        key: WritableConfigKey,

        /// Configuration value.
        value: String,
    },

    /// Unset the provided configuration key
    Unset {
        /// Configuration key.
        #[structopt(help = HELP_VALID_UNSETTABLE_KEYS)]
        key: UnsettableConfigKey,
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
        // Temporary implementation
        // - should return a specific command, not self.
        // - see certificate.rs for an example
        Ok(self.into_boxed())
    }
}

impl Command for ConfigCmd {
    fn description(&self) -> String {
        match self {
            ConfigCmd::Get { key } => {
                format!("get the configuration value for key: {}", key.as_str())
            }
            ConfigCmd::Set { key, value } => format!(
                "set the configuration key: {} with value: {}.",
                key.as_str(),
                value
            ),
            ConfigCmd::Unset { key } => {
                format!("unset the configuration value for key: {}", key.as_str())
            }
            ConfigCmd::List { .. } => String::from("list the configuration keys and values"),
        }
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let mut config = TEdgeConfig::from_default_config()?;
        let mut config_updated = false;

        match self {
            ConfigCmd::Get { key } => match key.get_config_value(&config) {
                Ok(value) => println!("{}", value),
                Err(ConfigSettingError::ConfigNotSet { key }) => {
                    println!("The provided config key: '{}' is not set", key)
                }
                Err(err) => return Err(err.into()),
            },
            ConfigCmd::Set { key, value } => {
                key.set_config_value(&mut config, value.into())?;
                config_updated = true;
            }
            ConfigCmd::Unset { key } => {
                key.unset_config_value(&mut config)?;
                config_updated = true;
            }
            ConfigCmd::List { is_all, is_doc } => match is_doc {
                true => print_config_doc(),
                false => print_config_list(&config, *is_all)?,
            },
        }

        if config_updated {
            config.write_to_default_config()?;
        }
        Ok(())
    }
}

fn print_config_doc() {
    for config_key in ConfigKey::all() {
        println!("{:<30} {}", config_key.key, config_key.description);
    }
}

fn print_config_list(config: &TEdgeConfig, all: bool) -> Result<(), ConfigError> {
    let mut keys_without_values: Vec<&str> = Vec::new();
    for config_key in ConfigKey::all() {
        match config_key {
            ConfigKey {
                key,
                getter: Some(getter),
                ..
            } => match getter.get_string(config) {
                Ok(value) => println!("{}={}", key, value),
                Err(ConfigSettingError::ConfigNotSet { .. }) => keys_without_values.push(key),
                Err(err) => return Err(err.into()),
            },
            _ => {}
        }
    }

    if all && !keys_without_values.is_empty() {
        println!();
        for key in keys_without_values {
            println!("{}=", key);
        }
    }
    Ok(())
}
