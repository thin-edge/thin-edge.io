use crate::cli::config::config_keys::*;
use crate::command::{Command, ExecutionContext};
use tedge_config::*;

pub struct ListConfigCommand {
    pub is_all: bool,
    pub is_doc: bool,
    pub config: TEdgeConfig,
}

impl Command for ListConfigCommand {
    fn description(&self) -> String {
        "list the configuration keys and values".into()
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        match self.is_doc {
            true => print_config_doc(),
            false => print_config_list(&self.config, self.is_all)?,
        }

        Ok(())
    }
}

fn print_config_doc() {
    for config_key in ConfigKeyRegistry::all().iter() {
        println!("{:<30} {}", config_key.key, config_key.description);
    }
}

fn print_config_list(config: &TEdgeConfig, all: bool) -> Result<(), ConfigError> {
    let mut keys_without_values: Vec<&str> = Vec::new();
    for config_key in ConfigKeyRegistry::all().iter() {
        match (config_key.get_value)(config) {
            Ok(value) => println!("{}={}", config_key.key, value),
            Err(ConfigSettingError::ConfigNotSet { .. }) => {
                keys_without_values.push(config_key.key)
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
