mod request;
mod response;
mod task;

pub use request::*;
pub use response::*;

use crate::task::RunCommandTask;
use async_trait::async_trait;
use tedge_actors::{Actor, DevNull, Reactor, Recipient, RuntimeError, Task};

pub struct CommandRunner;

#[async_trait]
impl Actor for CommandRunner {
    type Config = ();
    type Input = CommandRequest;
    type Output = CommandStatus;
    type Producer = DevNull;
    type Reactor = CommandRunner;

    fn try_new(_config: &Self::Config) -> Result<Self, RuntimeError> {
        Ok(CommandRunner)
    }

    async fn start(self) -> Result<(Self::Producer, Self::Reactor), RuntimeError> {
        Ok((DevNull, CommandRunner))
    }
}

#[async_trait]
impl Reactor<CommandRequest, CommandStatus> for CommandRunner {
    async fn react(
        &mut self,
        request: CommandRequest,
        output: &mut Recipient<CommandStatus>,
    ) -> Result<Option<Box<dyn Task>>, RuntimeError> {
        let task = match request {
            CommandRequest::RunCommand(command) => RunCommandTask::new(command, output.clone()),
        };
        Ok(Some(Box::new(task)))
    }
}
