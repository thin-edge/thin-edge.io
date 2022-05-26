use crate::RunCommand;
use std::process::ExitStatus;
use tedge_actors::message_type;
use tedge_actors::Message;

message_type!(CommandStatus[CommandLaunched,LaunchError,CommandDone]);

#[derive(Clone, Debug)]
pub struct CommandLaunched {
    pub command: RunCommand,
    pub process_id: u32,
}

#[derive(Clone, Debug)]
pub struct LaunchError {
    pub command: RunCommand,
    pub error: String,
}

#[derive(Clone, Debug)]
pub struct CommandDone {
    pub command: RunCommand,
    pub status: ExitStatus,
}
