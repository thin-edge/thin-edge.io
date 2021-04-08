use crate::command::{BuildCommand, Command, ExecutionContext};
use crate::config::*;
use structopt::StructOpt;

/// Wrapper type for configuration keys.
#[derive(Debug, Clone)]
pub struct ConfigKey(pub String);

impl ConfigKey {
    fn as_str(&self) -> &str {
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
    fn as_str(&self) -> &str {
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

#[derive(StructOpt, Debug)]
pub enum ConfigCmd {
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

    /// Get the value of the provided configuration key
    Get {
        /// Configuration key.
        #[structopt(help = TEdgeConfig::valid_keys_help_message_for_get())]
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
        // Temporary implementation
        // - should return a specific command, not self.
        // - see certificate.rs for an example
        Ok(self.into_boxed())
    }
}

impl Command for ConfigCmd {
    fn description(&self) -> String {
        match self {
            ConfigCmd::Set { key, value } => format!(
                "set the configuration key: {} with value: {}.",
                key.as_str(),
                value
            ),
            ConfigCmd::Get { key } => {
                format!("get the configuration value for key: {}", key.as_str())
            }
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
            ConfigCmd::Get { key } => match config.get_config_value(key.as_str())? {
                None => println!("The provided config key: '{}' is not set", key.as_str()),
                Some(value) => println!("{}", value),
            },
            ConfigCmd::Set { key, value } => {
                config.set_config_value(key.as_str(), value.to_string())?;
                config_updated = true;
            }
            ConfigCmd::Unset { key } => {
                config.unset_config_value(key.as_str())?;
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

fn print_config_list(config: &TEdgeConfig, all: bool) -> Result<(), ConfigError> {
    let mut keys_without_values: Vec<&str> = Vec::new();
    for key in TEdgeConfig::valid_keys() {
        let opt = config.get_config_value(key)?;
        match opt {
            Some(value) => println!("{}={}", key, value),
            None => keys_without_values.push(key),
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

fn print_config_doc() {
    for key in TEdgeConfig::valid_keys() {
        // key is pre-defined surely
        let desc = TEdgeConfig::get_key_properties(key).unwrap().description;
        println!("{:<30} {}", key, desc);
    }
}
