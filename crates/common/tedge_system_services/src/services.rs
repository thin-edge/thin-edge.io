use std::fmt;
use tedge_config::tedge_toml::ProfileName;

/// A system service that can optionally have a profile.
/// When a profile is specified, the service is formatted as "name@profile".
#[derive(Debug, Copy, Clone)]
pub struct SystemService<'a> {
    pub name: &'a str,
    pub profile: Option<&'a ProfileName>,
}

impl<'a> SystemService<'a> {
    /// Creates a new service without a profile.
    pub const fn new(name: &'a str) -> Self {
        Self {
            name,
            profile: None,
        }
    }
}

impl fmt::Display for SystemService<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.profile {
            Some(profile) => write!(f, "{}@{}", self.name, profile),
            None => write!(f, "{}", self.name),
        }
    }
}
