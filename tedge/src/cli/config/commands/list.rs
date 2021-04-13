use crate::cli::config::config_key::*;
use crate::command::{Command, ExecutionContext};
use crate::config::ConfigError;
use tedge_config::*;

pub struct ListConfigCommand {
    pub is_all: bool,
    pub is_doc: bool,
    pub config: TEdgeConfig,
    pub config_keys: Vec<ConfigKey>,
}

impl Command for ListConfigCommand {
    fn description(&self) -> String {
        "list the configuration keys and values".into()
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        if self.is_doc {
            print_config_doc(&self.config_keys);
        } else {
            print_config_list(&self.config_keys, &self.config, self.is_all)?;
        }

        Ok(())
    }
}

fn print_config_list(
    config_keys: &[ConfigKey],
    config: &TEdgeConfig,
    all: bool,
) -> Result<(), ConfigError> {
    let mut keys_without_values: Vec<String> = Vec::new();
    for config_key in config_keys {
        match (config_key.get)(config) {
            Ok(value) => {
                println!("{}={}", config_key.key, value);
            }
            Err(tedge_config::ConfigSettingError::ConfigNotSet { .. }) => {
                keys_without_values.push(config_key.key.into());
            }
            Err(err) => return Err(err.into()),
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

fn print_config_doc(config_keys: &[ConfigKey]) {
    for config_key in config_keys {
        println!("{:<30} {}", config_key.key, config_key.description);
    }
}
