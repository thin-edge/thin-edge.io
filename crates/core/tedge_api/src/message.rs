use crate::plugin::Message;

/// A message to tell the core to stop thin-edge
#[derive(Debug)]
pub struct StopCore;

impl Message for StopCore {}

crate::make_message_bundle!(pub struct CoreMessages(StopCore));
