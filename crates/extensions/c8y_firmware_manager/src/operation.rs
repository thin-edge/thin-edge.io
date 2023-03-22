use crate::error::FirmwareManagementError;

use std::fs;
use std::path::Path;
use tedge_utils::file::create_file_with_mode;
use tedge_utils::file::overwrite_file;

#[derive(Debug, Eq, PartialEq, Default, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FirmwareOperationEntry {
    pub operation_id: String,
    pub child_id: String,
    pub name: String,
    pub version: String,
    pub server_url: String,
    pub file_transfer_url: String,
    pub sha256: String,
    pub attempt: usize,
}

impl FirmwareOperationEntry {
    pub fn create_status_file(&self, firmware_dir: &Path) -> Result<(), FirmwareManagementError> {
        let path = firmware_dir.join(&self.operation_id);
        let content = serde_json::to_string(self)?;
        create_file_with_mode(path, Some(content.as_str()), 0o644)
            .map_err(FirmwareManagementError::FromFileError)
    }

    pub fn overwrite_file(&self, firmware_dir: &Path) -> Result<(), FirmwareManagementError> {
        let path = firmware_dir.join(&self.operation_id);
        let content = serde_json::to_string(self)?;
        overwrite_file(&path, &content).map_err(FirmwareManagementError::FromFileError)
    }

    pub fn increment_attempt(self) -> Self {
        Self {
            attempt: self.attempt + 1,
            ..self
        }
    }

    pub fn read_from_file(path: &Path) -> Result<Self, FirmwareManagementError> {
        let bytes = fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(FirmwareManagementError::FromSerdeJsonError)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct OperationKey {
    pub child_id: String,
    pub operation_id: String,
}

impl OperationKey {
    pub fn new(child_id: &str, operation_id: &str) -> Self {
        Self {
            child_id: child_id.to_string(),
            operation_id: operation_id.to_string(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ActiveOperationState {
    Pending,
    Executing,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tedge_test_utils::fs::TempTedgeDir;

    #[test]
    fn read_entry_from_file() {
        let op_id = "op-id";
        let content = json!({
          "operation_id": op_id,
          "child_id": "child-id",
          "name": "fw-name",
          "version": "fw-version",
          "server_url": "server-url",
          "file_transfer_url": "file-transfer-url",
          "sha256": "abcd1234",
          "attempt": 1
        })
        .to_string();

        let ttd = TempTedgeDir::new();
        ttd.dir("firmware").file(op_id).with_raw_content(&content);
        let file_path = ttd.path().join("firmware").join(op_id);

        let entry = FirmwareOperationEntry::read_from_file(&file_path).unwrap();
        let expected_entry = FirmwareOperationEntry {
            operation_id: "op-id".to_string(),
            child_id: "child-id".to_string(),
            name: "fw-name".to_string(),
            version: "fw-version".to_string(),
            server_url: "server-url".to_string(),
            file_transfer_url: "file-transfer-url".to_string(),
            sha256: "abcd1234".to_string(),
            attempt: 1,
        };
        assert_eq!(entry, expected_entry);
    }
}
