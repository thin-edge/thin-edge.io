use crate::software::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct SoftwareRequest {
    pub id: String,
    pub operation: SoftwareOperation,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SoftwareResponse {
    pub id: String,
    pub status: SoftwareOperationStatus,
}
