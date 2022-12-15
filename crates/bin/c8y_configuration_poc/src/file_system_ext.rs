use std::path::PathBuf;
use tedge_actors::{DynSender, RuntimeError, RuntimeHandle};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatcherConfig {
    pub directory: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileEvent {
    //FileDeleted(PathBuf),
    //FileCreated(PathBuf),
    //DirectoryDeleted(PathBuf),
    //DirectoryCreated(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileRequest {}

pub async fn new_watcher(
    _runtime: &mut RuntimeHandle,
    _config: WatcherConfig,
    _client: DynSender<FileEvent>,
) -> Result<DynSender<FileRequest>, RuntimeError> {
    todo!()
}
