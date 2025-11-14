use super::TEdgeCli;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::ConfigError;
use clap::CommandFactory;
use std::io;
use tedge_config::TEdgeConfig;

#[derive(clap::ValueEnum, Clone, Copy, Debug, strum_macros::Display)]
#[strum(serialize_all = "snake_case")]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl From<Shell> for clap_complete::Shell {
    fn from(value: Shell) -> Self {
        match value {
            Shell::Bash => Self::Bash,
            Shell::Zsh => Self::Zsh,
            Shell::Fish => Self::Fish,
        }
    }
}

impl From<Shell> for Box<dyn clap_complete::env::EnvCompleter> {
    fn from(value: Shell) -> Self {
        use clap_complete::env;
        match value {
            Shell::Bash => Box::new(env::Bash),
            Shell::Zsh => Box::new(env::Zsh),
            Shell::Fish => Box::new(env::Fish),
        }
    }
}

#[async_trait::async_trait]
impl BuildCommand for Shell {
    async fn build_command(self, _: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        Ok(Box::new(CompletionsCmd { shell: self }))
    }
}

#[derive(Clone, Copy, Debug)]
struct CompletionsCmd {
    shell: Shell,
}

#[async_trait::async_trait]
impl Command for CompletionsCmd {
    fn description(&self) -> String {
        format!("generate shell tab completion script for {}", self.shell)
    }

    async fn execute(&self, _: TEdgeConfig) -> Result<(), super::log::MaybeFancy<anyhow::Error>> {
        let cmd = TEdgeCli::command();
        let completer = std::env::current_exe().unwrap();
        let completer = completer.to_str().unwrap();
        Box::<dyn clap_complete::env::EnvCompleter>::from(self.shell)
            .write_registration(
                "COMPLETE",
                "tedge",
                cmd.get_bin_name().unwrap_or_else(|| cmd.get_name()),
                completer,
                &mut io::stdout(),
            )
            .unwrap();
        Ok(())
    }
}
