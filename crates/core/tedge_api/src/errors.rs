#[derive(Debug)]
pub struct PluginConfigurationError {
    kind: PluginConfigurationErrorKind,
    span: Option<(usize, usize)>,
}

impl PluginConfigurationError {
    pub const fn new(span: (usize, usize), kind: PluginConfigurationErrorKind) -> Self {
        Self {
            span: Some(span),
            kind,
        }
    }

    /// Get a reference to the plugin configuration error's kind.
    pub fn kind(&self) -> &PluginConfigurationErrorKind {
        &self.kind
    }

    /// Get the plugin configuration error's span.
    pub fn span(&self) -> Option<(usize, usize)> {
        self.span
    }
}

impl From<PluginConfigurationErrorKind> for PluginConfigurationError {
    fn from(kind: PluginConfigurationErrorKind) -> Self {
        Self { span: None, kind }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum PluginConfigurationErrorKind {}

#[derive(Debug)]
pub struct PluginError {
    kind: PluginErrorKind,
}

impl PluginError {
    pub const fn new(kind: PluginErrorKind) -> Self {
        Self { kind }
    }

    /// Get a reference to the plugin error's kind.
    pub fn kind(&self) -> &PluginErrorKind {
        &self.kind
    }
}

impl From<PluginErrorKind> for PluginError {
    fn from(kind: PluginErrorKind) -> Self {
        Self { kind }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum PluginErrorKind {}
