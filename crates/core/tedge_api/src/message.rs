use crate::plugin::Message;

#[derive(Debug)]
pub struct StopCore;

impl Message for StopCore {}

crate::make_message_bundle!(pub struct CoreMessages(StopCore));
