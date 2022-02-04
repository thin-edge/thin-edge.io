#[derive(Debug)]
pub struct PluginConfigurationError {
    kind: PluginConfigurationErrorKind,
    span: Option<(usize, usize)>,
}

impl PluginConfigurationError {
    pub fn new(span: (usize, usize), kind: PluginConfigurationErrorKind) -> Self {
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
