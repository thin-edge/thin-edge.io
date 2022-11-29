use std::path::PathBuf;
use tedge_actors::{Recipient, RuntimeHandle};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatcherConfig {
    pub directory: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileEvent {
    FileDeleted(PathBuf),
    FileCreated(PathBuf),
    DirectoryDeleted(PathBuf),
    DirectoryCreated(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileRequest {}

pub fn new_watcher(
    runtime: &mut RuntimeHandle,
    config: WatcherConfig,
    recipient: Recipient<FileEvent>,
) -> Recipient<FileRequest> {
    todo!()
}
