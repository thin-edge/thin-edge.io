use crate::address::Address;

/// A message to be handled by a plugin
#[derive(Debug)]
pub struct Message {
    origin: Address,
    kind: MessageKind,
}

impl Message {
    pub fn new(origin: Address, kind: MessageKind) -> Self {
        Self { origin, kind }
    }

    /// Get a reference to the plugin message's kind.
    pub fn kind(&self) -> &MessageKind {
        &self.kind
    }

    /// Get a reference to the plugin message's origin.
    pub fn origin(&self) -> &Address {
        &self.origin
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum MessageKind {
    /// The plugin is being asked if it is currently able to respond
    /// to requests. It is meant to reply with `CoreMessageKind` stating
    /// its status.
    CheckReadyness,
    SignalPluginState { state: String },
}
