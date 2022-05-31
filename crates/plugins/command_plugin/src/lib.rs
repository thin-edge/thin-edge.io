mod request;
mod response;
mod task;

pub use request::*;
pub use response::*;

use crate::task::RunCommandTask;
use async_trait::async_trait;
use tedge_actors::{Actor, Recipient, RuntimeError, RuntimeHandler};

pub struct CommandRunner;

#[async_trait]
impl Actor for CommandRunner {
    type Config = ();
    type Input = CommandRequest;
    type Output = CommandStatus;

    fn try_new(_config: Self::Config) -> Result<Self, RuntimeError> {
        Ok(CommandRunner)
    }

    async fn start(
        &mut self,
        _runtime: RuntimeHandler,
        _output: Recipient<CommandStatus>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn react(
        &mut self,
        request: CommandRequest,
        runtime: &mut RuntimeHandler,
        output: &mut Recipient<CommandStatus>,
    ) -> Result<(), RuntimeError> {
        let task = match request {
            CommandRequest::RunCommand(command) => RunCommandTask::new(command, output.clone()),
        };
        runtime.spawn(task).await
    }
}
