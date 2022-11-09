use std::path::{Path, PathBuf};

use c8y_api::smartrest::operations::is_valid_operation_name;
use serde::{Deserialize, Serialize};
use tedge_utils::notify::FsEvent;

#[derive(Serialize, Deserialize, Debug)]
pub enum EventType {
    Add,
    Remove,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DiscoverOp {
    pub ops_dir: PathBuf,
    pub event_type: EventType,
    pub operation_name: String,
}

#[derive(thiserror::Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum DynamicDiscoverOpsError {
    #[error("A non-UTF8 path cannot be parsed as an operation: {0:?}")]
    NotAnOperation(PathBuf),

    #[error(transparent)]
    EventError(#[from] std::io::Error),
}

pub fn process_inotify_events(
    path: &Path,
    mask: FsEvent,
) -> Result<Option<DiscoverOp>, DynamicDiscoverOpsError> {
    let operation_name = path
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .ok_or_else(|| DynamicDiscoverOpsError::NotAnOperation(path.to_path_buf()))?;

    let parent_dir = path
        .parent()
        .ok_or_else(|| DynamicDiscoverOpsError::NotAnOperation(path.to_path_buf()))?;

    if is_valid_operation_name(operation_name) {
        match mask {
            FsEvent::FileDeleted => Ok(Some(DiscoverOp {
                ops_dir: parent_dir.to_path_buf(),
                event_type: EventType::Remove,
                operation_name: operation_name.to_string(),
            })),
            FsEvent::FileCreated | FsEvent::Modified => Ok(Some(DiscoverOp {
                ops_dir: parent_dir.to_path_buf(),
                event_type: EventType::Add,
                operation_name: operation_name.to_string(),
            })),
            _ => Ok(None),
        }
    } else {
        Ok(None)
    }
}
