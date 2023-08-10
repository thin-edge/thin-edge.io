use crate::command::Command;
use tedge_config::TEdgeConfigRepository;
use tedge_config::WritableKey;

pub struct UnsetConfigCommand {
    pub key: WritableKey,
    pub config_repository: TEdgeConfigRepository,
}

impl Command for UnsetConfigCommand {
    fn description(&self) -> String {
        format!("unset the configuration value for '{}'", self.key)
    }

    fn execute(&self) -> anyhow::Result<()> {
        self.config_repository.update_toml_new(&|dto| {
            dto.unset_key(self.key);
            Ok(())
        })?;
        Ok(())
    }
}
