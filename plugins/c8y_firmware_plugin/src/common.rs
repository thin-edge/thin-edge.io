use crate::download::DownloadFirmwareStatusMessage;
use crate::error::FirmwareManagementError;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::Message;
use mqtt_channel::SinkExt;
use mqtt_channel::Topic;
use mqtt_channel::UnboundedSender;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use tedge_utils::file::create_file_with_user_group;
use tedge_utils::file::get_gid_by_name;
use tedge_utils::file::get_metadata;
use tedge_utils::file::get_uid_by_name;
use tedge_utils::file::overwrite_file;

#[cfg(not(test))]
pub const FIRMWARE_OPERATION_DIR_PATH: &str = "/var/tedge/firmware";
#[cfg(test)]
pub const FIRMWARE_OPERATION_DIR_PATH: &str = "/tmp/firmware";

pub struct PersistentStore;
impl PersistentStore {
    pub fn get_dir_path() -> PathBuf {
        PathBuf::from(FIRMWARE_OPERATION_DIR_PATH)
    }

    pub fn get_file_path(op_id: &str) -> PathBuf {
        PathBuf::from(FIRMWARE_OPERATION_DIR_PATH).join(op_id)
    }

    pub fn has_expected_permission(op_id: &str) -> Result<(), FirmwareManagementError> {
        let path = Self::get_file_path(op_id);

        let metadata = get_metadata(path.as_path())?;
        let file_uid = metadata.uid();
        let file_gid = metadata.gid();
        let tedge_uid = get_uid_by_name("tedge")?;
        let tedge_gid = get_gid_by_name("tedge")?;
        let root_uid = get_uid_by_name("root")?;
        let root_gid = get_gid_by_name("root")?;

        if (file_uid == tedge_uid || file_uid == root_uid)
            && (file_gid == tedge_gid || file_gid == root_gid)
            && format!("{:o}", metadata.permissions().mode()).contains("644")
        {
            Ok(())
        } else {
            Err(FirmwareManagementError::InvalidFilePermission {
                id: op_id.to_string(),
            })
        }
    }
}

#[derive(Debug, Eq, PartialEq, Default, Clone, Deserialize, Serialize)]
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
    pub fn create_file(&self) -> Result<(), FirmwareManagementError> {
        let path = PersistentStore::get_file_path(&self.operation_id);
        create_parent_dirs(&path)?;
        let content = serde_json::to_string(self)?;
        create_file_with_user_group(path, "tedge", "tedge", 0o644, Some(content.as_str()))
            .map_err(FirmwareManagementError::FromFileError)
    }

    pub fn overwrite_file(&self) -> Result<(), FirmwareManagementError> {
        let path = PersistentStore::get_file_path(&self.operation_id);
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

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ActiveOperationState {
    Pending,
    Executing,
}

pub async fn mark_pending_firmware_operation_failed(
    mut mqtt_publisher: UnboundedSender<Message>,
    child_id: impl ToString,
    op_state: ActiveOperationState,
    failure_reason: impl ToString,
) -> Result<(), anyhow::Error> {
    let c8y_child_topic =
        Topic::new_unchecked(&C8yTopic::ChildSmartRestResponse(child_id.to_string()).to_string());

    let executing_msg = Message::new(
        &c8y_child_topic,
        DownloadFirmwareStatusMessage::status_executing()?,
    );
    let failed_msg = Message::new(
        &c8y_child_topic,
        DownloadFirmwareStatusMessage::status_failed(failure_reason.to_string())?,
    );

    if op_state == ActiveOperationState::Pending {
        mqtt_publisher.send(executing_msg).await?;
    }

    mqtt_publisher.send(failed_msg).await?;

    Ok(())
}

// TODO! Move to common crate
pub fn create_parent_dirs(path: &Path) -> Result<(), FirmwareManagementError> {
    if let Some(dest_dir) = path.parent() {
        if !dest_dir.exists() {
            fs::create_dir_all(dest_dir)?;
        }
    }
    Ok(())
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
