use crate::cli::config::config_keys::*;
use crate::command::{Command, ExecutionContext};
use tedge_config::*;

pub struct SetConfigCommand {
    pub key: ReadWriteConfigKey,
    pub value: String,
    pub config_manager: TEdgeConfigManager,
}

impl Command for SetConfigCommand {
    fn description(&self) -> String {
        format!(
            "set the configuration key: {} with value: {}.",
            self.key.as_str(),
            self.value
        )
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        // XXX: change to execute(self)
        let mut config = self.config_manager.from_default_config()?;

        self.key
            .set_config_value(&mut config, self.value.clone().into())?;
        config.persist()?;
        Ok(())
    }
}
