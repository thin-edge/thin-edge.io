use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "error")]
pub enum Error {
    #[error("Invalid request: {description}")]
    InvalidRequest { description: String },

    #[error("Service error: {0}")]
    ServiceError(String),
}
