use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tedge_utils::fs_notify::FileEvent;
use tracing::log::warn;

const C8Y_PREFIX: &str = "c8y_";

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

/// depending on which editor you use, temporary files could be created.
/// this `operation_name_is_valid` fn will ensure that only files created
/// that start with `c8y_` and contain alphabetic chars are allowed.
fn operation_name_is_valid(operation: &str) -> bool {
    let c8y_index = 4;
    let (prefix, clipped_operation) = operation.split_at(c8y_index);

    if prefix.eq(C8Y_PREFIX) {
        clipped_operation.chars().all(|c| c.is_ascii_alphabetic())
    } else {
        false
    }
}

pub fn process_inotify_events(
    path: &Path,
    mask: FileEvent,
) -> Result<Option<DiscoverOp>, DynamicDiscoverOpsError> {
    let operation_name = path
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .ok_or_else(|| DynamicDiscoverOpsError::NotAnOperation(path.to_path_buf()))?;

    let parent_dir = path
        .parent()
        .ok_or_else(|| DynamicDiscoverOpsError::NotAnOperation(path.to_path_buf()))?;

    if operation_name_is_valid(operation_name) {
        match mask {
            FileEvent::Deleted => Ok(Some(DiscoverOp {
                ops_dir: parent_dir.to_path_buf(),
                event_type: EventType::Remove,
                operation_name: operation_name.to_string(),
            })),
            FileEvent::Created => Ok(Some(DiscoverOp {
                ops_dir: parent_dir.to_path_buf(),
                event_type: EventType::Add,
                operation_name: operation_name.to_string(),
            })),
            mask => {
                warn!("Did nothing for mask: {}", mask);
                Ok(None)
            }
        }
    } else {
        Ok(None)
    }
}
