use crate::cli::config::config_keys::*;
use crate::command::{Command, ExecutionContext};
use tedge_config::*;

pub struct UnsetConfigCommand {
    pub key: UnsettableConfigKey,
    pub config_manager: TEdgeConfigManager,
}

impl Command for UnsetConfigCommand {
    fn description(&self) -> String {
        format!(
            "unset the configuration value for key: {}",
            self.key.as_str()
        )
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        // XXX: We cannot call persist, because persist requires &mut
        let mut config = self.config_manager.from_default_config()?;

        self.key.unset_config_value(&mut config)?;
        config.persist()?;
        Ok(())
    }
}
