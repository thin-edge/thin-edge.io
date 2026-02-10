use tedge_config::TEdgeConfig;

use crate::command::BuildCommand;
use crate::command::Command;
use crate::ConfigError;

mod common;
mod inspect;
mod test_command;

#[derive(clap::Subcommand, Debug)]
pub enum BridgeCmd {
    Inspect(inspect::BridgeInspectCmd),
    Test(test_command::BridgeTestCmd),
}
#[async_trait::async_trait]
impl BuildCommand for BridgeCmd {
    async fn build_command(self, _config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            Self::Inspect(cmd) => Ok(cmd.into_boxed()),
            Self::Test(cmd) => Ok(cmd.into_boxed()),
        }
    }
}
