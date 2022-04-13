use miette::Diagnostic;
use thiserror::Error;

pub type PluginError = miette::Report;

#[derive(Error, Debug, Diagnostic)]
pub enum DirectoryError {
    #[error("Plugin named '{}' not found", .0)]
    PluginNameNotFound(String),

    #[error("Plugin '{}' does not support the following message types: {}", .0 ,.1.join(","))]
    PluginDoesNotSupport(String, Vec<&'static str>),
}
