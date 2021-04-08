use crate::command::{Command, ExecutionContext};
use crate::config::*;

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
        if self.is_doc {
            print_config_doc()
        } else {
            print_config_list(&self.config, self.is_all)?;
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
