use std::ffi::OsString;
use tedge_actors::message_type;
use tedge_actors::Message;

message_type!(CommandRequest[RunCommand]);

#[derive(Clone, Debug)]
pub struct RunCommand {
    pub program: OsString,
    pub arguments: Vec<OsString>,
}

impl Message for RunCommand {}
