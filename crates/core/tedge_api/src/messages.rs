use crate::address::Address;

/// A message to be handled by a plugin
#[derive(Debug)]
pub struct Message {
    origin: Address,
    kind: MessageKind,
    id: uuid::Uuid,
}

impl Message {
    pub fn new(origin: Address, kind: MessageKind) -> Self {
        Self {
            origin,
            kind,
            id: uuid::Uuid::new_v4(),
        }
    }

    /// Get the message id
    pub fn id(&self) -> &uuid::Uuid {
        &self.id
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
    /// A reply to the message with the contained ID
    Reply(uuid::Uuid),

    /// The plugin is being asked if it is currently able to respond
    /// to requests. It is meant to reply with `CoreMessageKind` stating
    /// its status.
    CheckReadyness,
    SignalPluginState {
        state: String,
    },
}

#[cfg(test)]
mod tests {
    use static_assertions::assert_impl_all;

    use super::Message;

    assert_impl_all!(Message: Send);
}
