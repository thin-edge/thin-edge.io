use crate::{CommandDone, CommandLaunched, CommandStatus, LaunchError, RunCommand};
use async_process::Command;
use async_trait::async_trait;
use std::io::Error;
use std::process::ExitStatus;
use tedge_actors::{Recipient, RuntimeError, Task};

pub struct RunCommandTask {
    command: RunCommand,
    output: Recipient<CommandStatus>,
}

impl RunCommandTask {
    pub fn new(command: RunCommand, output: Recipient<CommandStatus>) -> RunCommandTask {
        RunCommandTask { command, output }
    }

    fn system_command(&self) -> Command {
        let command = self.command.clone();
        let mut system_command = Command::new(command.program);
        system_command.args(command.arguments).kill_on_drop(true);
        system_command
    }

    async fn notify_error(&mut self, err: Error) -> Result<(), RuntimeError> {
        Ok(self
            .output
            .send_message(
                LaunchError {
                    command: self.command.clone(),
                    error: format!("{}", err).to_string(),
                }
                .into(),
            )
            .await?)
    }

    async fn notify_launched(&mut self, process_id: u32) -> Result<(), RuntimeError> {
        Ok(self
            .output
            .send_message(
                CommandLaunched {
                    command: self.command.clone(),
                    process_id,
                }
                .into(),
            )
            .await?)
    }

    async fn notify_done(&mut self, status: ExitStatus) -> Result<(), RuntimeError> {
        Ok(self
            .output
            .send_message(
                CommandDone {
                    command: self.command.clone(),
                    status,
                }
                .into(),
            )
            .await?)
    }
}

#[async_trait]
impl Task for RunCommandTask {
    async fn run(mut self: Box<Self>) -> Result<(), RuntimeError> {
        let mut command = self.system_command();

        match command.spawn() {
            Err(err) => {
                self.notify_error(err).await?;
            }
            Ok(mut child) => {
                self.notify_launched(child.id()).await?;

                match child.status().await {
                    Err(err) => {
                        self.notify_error(err).await?;
                    }
                    Ok(status) => {
                        self.notify_done(status).await?;
                    }
                }
            }
        }

        Ok(())
    }
}
