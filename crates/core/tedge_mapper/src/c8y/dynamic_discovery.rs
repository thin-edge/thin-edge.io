use std::{ffi::OsString, path::PathBuf};

use inotify::{Event, EventMask, Inotify, WatchMask};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum EventType {
    ADD,
    REMOVE,
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
    #[error("No Operations directory found: {0}")]
    NoOperations(String),

    #[error("No event")]
    NoEventName,

    #[error("Inotify event {0} not supported")]
    UnsupportedEvent(String),

    #[error(transparent)]
    EventError(#[from] std::io::Error),
}

pub fn create_inotify_watch(ops_dir: Option<PathBuf>) -> Result<Inotify, DynamicDiscoverOpsError> {
    match ops_dir {
        Some(dir) => {
            let mut inotify = Inotify::init()?;
            inotify
                .add_watch(dir.clone(), WatchMask::CLOSE_WRITE | WatchMask::DELETE)
                .map_err(|_| {
                    DynamicDiscoverOpsError::NoOperations(dir.to_string_lossy().to_string())
                })?;

            return Ok(inotify);
        }
        None => {
            return Err(DynamicDiscoverOpsError::NoOperations("None".to_string()));
        }
    }
}

pub fn create_inofity_event_stream(ops_dir: Option<PathBuf>) -> inotify::EventStream<[u8; 1024]> {
    let buffer = [0; 1024];
    let mut ino = create_inotify_watch(ops_dir).expect("Failed to create inotify watch");
    ino.event_stream(buffer)
        .expect("Failed to create the inotify event stream")
}

pub fn process_inotify_events(
    ops_dir: Option<PathBuf>,
    event: Result<Event<OsString>, std::io::Error>,
) -> Result<DiscoverOp, DynamicDiscoverOpsError> {
    match event {
        Ok(os_str) => {
            let operation_name = os_str
                .name
                .ok_or(DynamicDiscoverOpsError::NoEventName)?
                .to_str()
                .ok_or(DynamicDiscoverOpsError::NoEventName)?
                .to_string();

            let ops_dir = ops_dir
                .ok_or(DynamicDiscoverOpsError::NoOperations("None".to_string()))
                .expect("No operation directory");
            match os_str.mask {
                EventMask::DELETE => {
                    return Ok(DiscoverOp {
                        ops_dir,
                        event_type: EventType::REMOVE,
                        operation_name,
                    });
                }
                EventMask::CLOSE_WRITE => {
                    return Ok(DiscoverOp {
                        ops_dir,
                        event_type: EventType::ADD,
                        operation_name,
                    });
                }

                unsupported_event => {
                    return Err(DynamicDiscoverOpsError::UnsupportedEvent(format!(
                        "{:?}",
                        unsupported_event
                    )))
                }
            }
        }
        Err(e) => {
            return Err(DynamicDiscoverOpsError::EventError(e));
        }
    }
}

#[cfg(test)]
#[test]
fn create_inotify_with_non_existing_dir() {
    let err = create_inotify_watch(Some(PathBuf::from("/tmp/discover_ops"))).unwrap_err();
    assert_eq!(
        err.to_string(),
        "No Operations directory found: /tmp/discover_ops"
    );
}

#[test]
fn create_inotify_with_none() {
    let err = create_inotify_watch(None).unwrap_err();
    assert_eq!(err.to_string(), "No Operations directory found: None");
}

#[test]
fn create_inotify_with_right_directory() {
    use tempfile::TempDir;
    let dir = TempDir::new().unwrap().into_path();
    let res = create_inotify_watch(Some(dir));
    assert!(res.is_ok());
}
