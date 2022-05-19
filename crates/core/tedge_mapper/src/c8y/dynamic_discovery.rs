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
    #[error("Failed to add watch to directory: {0}")]
    FailedtoAddWatch(String),

    #[error("A non-UTF8 name cannot be used as an operation name: {0:?}")]
    NotAnOperationName(OsString),

    #[error(transparent)]
    EventError(#[from] std::io::Error),
}

pub fn create_inotify_watch(ops_dir: Option<PathBuf>) -> Result<Inotify, DynamicDiscoverOpsError> {
    let mut inotify = Inotify::init()?;
    match ops_dir {
        Some(dir) => {
            inotify
                .add_watch(dir.clone(), WatchMask::CLOSE_WRITE | WatchMask::DELETE)
                .map_err(|_| {
                    DynamicDiscoverOpsError::FailedtoAddWatch(dir.to_string_lossy().to_string())
                })?;
        }
        None => {}
    }
    Ok(inotify)
}

pub fn create_inofity_event_stream(ops_dir: Option<PathBuf>) -> inotify::EventStream<[u8; 1024]> {
    let buffer = [0; 1024];
    let mut ino = create_inotify_watch(ops_dir).expect("Failed to create inotify watch");
    ino.event_stream(buffer)
        .expect("Failed to create the inotify event stream")
}

pub fn process_inotify_events(
    ops_dir: PathBuf,
    event: Result<Event<OsString>, std::io::Error>,
) -> Option<DiscoverOp> {
    let mut operation: Option<DiscoverOp> = None;
    match event {
        Ok(os_str) => {
            if let Some(ops_name) = os_str.clone().name {
                let operation_name =
                    ops_name
                        .to_str()
                        .ok_or(DynamicDiscoverOpsError::NotAnOperationName(
                            ops_name.clone(),
                        ));

                operation = match operation_name {
                    Ok(ops_name) => match os_str.mask {
                        EventMask::DELETE => Some(DiscoverOp {
                            ops_dir,
                            event_type: EventType::REMOVE,
                            operation_name: ops_name.to_string(),
                        }),
                        EventMask::CLOSE_WRITE => Some(DiscoverOp {
                            ops_dir,
                            event_type: EventType::ADD,
                            operation_name: ops_name.to_string(),
                        }),
                        _ => None,
                    },
                    Err(e) => {
                        eprintln!("{}", e);
                        None
                    }
                };
            }
        }
        Err(e) => {
            eprintln!("{}", e);
        }
    }
    operation
}

#[cfg(test)]
#[test]
fn create_inotify_with_non_existing_dir() {
    let err = create_inotify_watch(Some(PathBuf::from("/tmp/discover_ops"))).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Failed to add watch to directory: /tmp/discover_ops"
    );
}

#[test]
fn create_inotify_with_right_directory() {
    use tempfile::TempDir;
    let dir = TempDir::new().unwrap().into_path();
    let res = create_inotify_watch(Some(dir));
    assert!(res.is_ok());
}
