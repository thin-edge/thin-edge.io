/// An address which could be either a target or source of messages
///
/// Nesting addresses allows to disambiguated between different kind of
/// sources and the way they have arrived here.
#[derive(Debug, Clone)]
pub struct Address {
    endpoint: EndpointKind,
    source: Option<Box<Address>>,
}

impl Address {
    pub fn new(endpoint: EndpointKind) -> Address {
        Self {
            endpoint,
            source: None,
        }
    }

    /// Get the original source of an `Address`
    pub fn origin(&self) -> &Address {
        if let Some(source) = self.source.as_ref() {
            source.origin()
        } else {
            self
        }
    }

    pub fn add_new_step(&self, endpoint: EndpointKind) -> Self {
        Self {
            endpoint,
            source: Some(Box::new(self.clone())),
        }
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
