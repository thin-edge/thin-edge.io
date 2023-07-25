use crate::command::Command;
use tedge_config::new::WritableKey;
use tedge_config::TEdgeConfigRepository;

pub struct SetConfigCommand {
    pub key: WritableKey,
    pub value: String,
    pub config_repository: TEdgeConfigRepository,
}

impl Command for SetConfigCommand {
    fn description(&self) -> String {
        format!(
            "set the configuration key: '{}' with value: {}.",
            self.key.as_str(),
            self.value
        )
    }

    fn execute(&self) -> anyhow::Result<()> {
        self.config_repository.update_toml_new(&|dto| {
            dto.try_update_str(self.key, &self.value)
                .map_err(|e| e.into())
        })?;
        Ok(())
    }
}
