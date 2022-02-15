use crate::address::Address;

/// A message to be handled by a plugin
#[derive(Debug)]
pub struct PluginMessage {
    origin: Address,
    kind: PluginMessageKind,
}

impl PluginMessage {
    pub fn new(origin: Address, kind: PluginMessageKind) -> Self {
        Self { origin, kind }
    }

    /// Get a reference to the plugin message's kind.
    pub fn kind(&self) -> &PluginMessageKind {
        &self.kind
    }

    /// Get a reference to the plugin message's origin.
    pub fn origin(&self) -> &Address {
        &self.origin
    }
}

#[derive(Debug)]
pub enum PluginMessageKind {
    /// The plugin is being asked if it is currently able to respond
    /// to requests. It is meant to reply with `CoreMessageKind` stating
    /// its status.
    CheckReadyness,
}
