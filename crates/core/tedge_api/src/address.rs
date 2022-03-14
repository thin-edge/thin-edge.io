/// An address which specifices either the unique name of a plugin or the core of ThinEdge
///
/// This is used in the [`Comms::send`](crate::plugin::Comms::send) method to send messages to
/// other plugins attached to the same core.
#[derive(Debug, Clone)]
pub struct Address {
    endpoint_kind: EndpointKind,
}

impl Address {
    /// Create a new address with the given destination/origin
    pub fn new(endpoint: EndpointKind) -> Address {
        Self {
            endpoint_kind: endpoint,
        }
    }

    /// Get the endpoint kind associated to this address
    pub fn endpoint_kind(&self) -> &EndpointKind {
        &self.endpoint_kind
    }
}

/// What kind of endpoint is it
#[derive(Debug, Clone)]
pub enum EndpointKind {
    /// The `tedge` core
    Core,
    /// A specific plugin
    Plugin { id: String },
}
