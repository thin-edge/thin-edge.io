use crate::cli::config::commands::*;
use crate::command::{BuildCommand, Command};
use crate::config::*;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub enum ConfigCmd {
    /// Get the value of the provided configuration key
    Get {
        /// Configuration key.
        #[structopt(help = TEdgeConfig::valid_keys_help_message_for_get())]
        key: ConfigKey,
    },

    /// Set or update the provided configuration key with the given value
    Set {
        /// Configuration key.
        #[structopt(help = TEdgeConfig::valid_keys_help_message_for_set())]
        key: WritableConfigKey,

        /// Configuration value.
        value: String,
    },

    /// Unset the provided configuration key
    Unset {
        /// Configuration key.
        #[structopt(help = TEdgeConfig::valid_keys_help_message_for_set())]
        key: WritableConfigKey,
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
    fn build_command(self, config: TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            ConfigCmd::Get { key } => Ok(GetConfigCommand { key, config }.into_boxed()),
            ConfigCmd::Set { key, value } => {
                Ok(SetConfigCommand { key, value, config }.into_boxed())
            }
            ConfigCmd::Unset { key } => Ok(UnsetConfigCommand { key, config }.into_boxed()),
            ConfigCmd::List { is_all, is_doc } => Ok(ListConfigCommand {
                is_all,
                is_doc,
                config,
            }
            .into_boxed()),
        }
    }
}

/// Wrapper type for configuration keys.
#[derive(Debug, Clone)]
pub struct ConfigKey(pub String);

impl ConfigKey {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::str::FromStr for ConfigKey {
    type Err = String;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        match TEdgeConfig::get_key_properties(key) {
            Some(_) => Ok(ConfigKey(key.into())),
            _ => Err(format!(
                "Invalid key `{}'. Valid keys are: [{}].",
                key,
                TEdgeConfig::valid_keys().join(", ")
            )),
        }
    }
}

/// Wrapper type for updatable (Read-Write mode) configuration keys.
#[derive(Debug, Clone)]
pub struct WritableConfigKey(pub String);

impl WritableConfigKey {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::str::FromStr for WritableConfigKey {
    type Err = String;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        match TEdgeConfig::get_key_properties(key) {
            Some(ConfigKeyProperties {
                mode: ConfigKeyMode::ReadWrite,
                ..
            }) => Ok(WritableConfigKey(key.into())),
            _ => {
                if key == DEVICE_ID {
                    Err(format!(
                        "Invalid key `{}'. Valid keys are: [{}].\n\
                Setting the device id is only allowed with tedge cert create. \
                To set 'device.id', use `tedge cert create --device-id <id>`.",
                        key,
                        TEdgeConfig::valid_writable_keys().join(", ")
                    ))
                } else {
                    Err(format!(
                        "Invalid key `{}'. Valid keys are: [{}].",
                        key,
                        TEdgeConfig::valid_writable_keys().join(", ")
                    ))
                }
            }
        }
    }
}
