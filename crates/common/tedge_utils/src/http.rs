/// The scheme used to connect to an HTTP server.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Protocol {
    Http,
    Https,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}
