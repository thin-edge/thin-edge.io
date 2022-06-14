pub use inotify_crate::EventMask;
pub use inotify_crate::WatchMask;
use inotify_crate::{EventStream, Inotify};
use std::path::Path;

pub fn inofity_file_watch_stream(
    config_file: &Path,

#[derive(thiserror::Error, Debug)]
pub enum InotifyError {
    #[error(transparent)]
    FromStdIo(#[from] std::io::Error),
}

pub fn inofity_stream(
    path: &Path,
    watch_flags: WatchMask,
) -> Result<EventStream<[u8; 1024]>, InotifyError> {
    let buffer = [0; 1024];
    let mut inotify = Inotify::init()?;

    inotify.add_watch(path, watch_flags)?;
    Ok(inotify.event_stream(buffer)?)
}
