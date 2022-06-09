pub use inotify_crate::EventMask;
pub use inotify_crate::WatchMask;
use inotify_crate::{EventStream, Inotify};
use std::path::Path;

pub fn inofity_file_watch_stream(
    config_file: &Path,
    watch_flags: WatchMask,
) -> Result<EventStream<[u8; 1024]>, anyhow::Error> {
    let buffer = [0; 1024];
    let mut inotify = Inotify::init().expect("Error while initializing inotify instance");

    inotify
        .add_watch(config_file, watch_flags)
        .expect("Failed to add file watch");
    Ok(inotify.event_stream(buffer)?)
}
