use crate::address::Address;

/// A message to be received by the `tedge` core component
///
/// It will be internally routed according to its destination
#[derive(Debug)]
pub struct CoreMessage {
    destination: Address,
    kind: CoreMessageKind,
}

impl CoreMessage {
    pub fn new(destination: Address, kind: CoreMessageKind) -> Self {
        Self { destination, kind }
    }

    /// Get a reference to the core message's kind.
    pub fn kind(&self) -> &CoreMessageKind {
        &self.kind
    }

    /// Get a reference to the core message's destination.
    pub fn destination(&self) -> &Address {
        &self.destination
    }
}

#[derive(Debug)]
pub enum CoreMessageKind {
    SendGenericMessage { message: Vec<u8> },
    SignalPluginState { state: String },
    // etc...
}

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
