pub struct PluginConfigurationError {
    pub kind: PluginConfigurationErrorKind,
    pub span: Option<(usize, usize)>,
}

impl PluginConfigurationError {
    pub fn new(span: (usize, usize), kind: PluginConfigurationErrorKind) -> Self {
        Self {
            span: Some(span),
            kind,
        }
    }
}

impl From<PluginConfigurationErrorKind> for PluginConfigurationError {
    fn from(kind: PluginConfigurationErrorKind) -> Self {
        Self { span: None, kind }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum PluginConfigurationErrorKind {}
