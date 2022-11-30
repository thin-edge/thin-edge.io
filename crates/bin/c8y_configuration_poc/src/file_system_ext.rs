use async_trait::async_trait;
use std::path::PathBuf;
use tedge_actors::{Recipient, RuntimeError, RuntimeHandle};

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

pub async fn new_watcher(
    runtime: &mut RuntimeHandle,
    config: WatcherConfig,
    client: Recipient<FileEvent>,
) -> Result<Recipient<FileRequest>, RuntimeError> {
    todo!()
}
