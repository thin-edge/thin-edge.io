use crate::address::Address;

/// A message to be received by the `tedge` core component
///
/// It will be internally routed according to its destination
#[derive(Debug)]
pub struct CoreMessage {
    destination: Address,
    content: CoreMessageKind,
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
    content: PluginMessageKind,
}

#[derive(Debug)]
pub enum PluginMessageKind {
    /// The plugin is being asked if it is currently able to respond
    /// to requests. It is meant to reply with `CoreMessageKind` stating
    /// its status.
    CheckReadyness,
}
