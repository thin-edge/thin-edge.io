use std::{ffi::OsString, path::PathBuf};

use inotify::{Event, EventMask, Inotify, WatchMask};
use serde::{Deserialize, Serialize};

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
    #[error("Failed to add watch to directory: {0}")]
    FailedtoAddWatch(String),

    #[error("A non-UTF8 name cannot be used as an operation name: {0:?}")]
    NotAnOperationName(OsString),

    #[error(transparent)]
    EventError(#[from] std::io::Error),
}

pub fn create_inotify_watch(ops_dir: PathBuf) -> Result<Inotify, DynamicDiscoverOpsError> {
    let mut inotify = Inotify::init()?;
    inotify
        .add_watch(ops_dir.clone(), WatchMask::CLOSE_WRITE | WatchMask::DELETE)
        .map_err(|_| {
            DynamicDiscoverOpsError::FailedtoAddWatch(ops_dir.to_string_lossy().to_string())
        })?;
    Ok(inotify)
}

pub fn create_inofity_event_stream(
    ops_dir: PathBuf,
) -> Result<inotify::EventStream<[u8; 1024]>, DynamicDiscoverOpsError> {
    let buffer = [0; 1024];
    let mut ino = create_inotify_watch(ops_dir)?;
    Ok(ino.event_stream(buffer)?)
}

pub fn process_inotify_events(
    ops_dir: PathBuf,
    event: Event<OsString>,
) -> Result<Option<DiscoverOp>, DynamicDiscoverOpsError> {
    if let Some(ops_name) = event.clone().name {
        let operation_name = ops_name
            .to_str()
            .ok_or_else(|| DynamicDiscoverOpsError::NotAnOperationName(ops_name.clone()));

        match operation_name {
            Ok(ops_name) => match event.mask {
                EventMask::DELETE => {
                    return Ok(Some(DiscoverOp {
                        ops_dir,
                        event_type: EventType::Remove,
                        operation_name: ops_name.to_string(),
                    }))
                }
                EventMask::CLOSE_WRITE => {
                    return Ok(Some(DiscoverOp {
                        ops_dir,
                        event_type: EventType::Add,
                        operation_name: ops_name.to_string(),
                    }))
                }
                _ => return Ok(None),
            },
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

#[cfg(test)]
#[test]
fn create_inotify_with_non_existing_dir() {
    let err = create_inotify_watch("/tmp/discover_ops".into()).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Failed to add watch to directory: /tmp/discover_ops"
    );
}

#[test]
fn create_inotify_with_right_directory() {
    use tedge_test_utils::fs::TempTedgeDir;
    let dir = TempTedgeDir::new();
    let res = create_inotify_watch(dir.path().to_path_buf());
    assert!(res.is_ok());
}
