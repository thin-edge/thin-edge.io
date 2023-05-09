use crate::command::Command;
use tedge_config::new::WritableKey;
use tedge_config::*;

pub struct UnsetConfigCommand {
    pub key: WritableKey,
    pub config_repository: TEdgeConfigRepository,
}

impl Command for UnsetConfigCommand {
    fn description(&self) -> String {
        format!("unset the configuration value for '{}'", self.key.as_str())
    }

    fn execute(&self) -> anyhow::Result<()> {
        self.config_repository
            .update_toml_new(&|dto| Ok(dto.unset_key(self.key)))?;
        Ok(())
    }
}
