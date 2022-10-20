use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    hash::Hash,
    path::{Path, PathBuf},
};

use async_stream::try_stream;
pub use futures::{pin_mut, Stream, StreamExt};
use inotify::{Event, EventMask, Inotify, WatchMask};
use nix::libc::c_int;
use strum_macros::Display;
use try_traits::default::TryDefault;

#[derive(Debug, Display, PartialEq, Eq, Clone, Hash, PartialOrd, Ord, Copy)]
pub enum FileEvent {
    Modified,
    Deleted,
    Created,
}

impl From<FileEvent> for WatchMask {
    fn from(value: FileEvent) -> Self {
        match value {
            FileEvent::Modified => WatchMask::MODIFY,
            FileEvent::Deleted => WatchMask::DELETE,
            FileEvent::Created => WatchMask::CREATE,
        }
    }
}

impl TryFrom<EventMask> for FileEvent {
    type Error = NotifyStreamError;

    fn try_from(value: EventMask) -> Result<Self, Self::Error> {
        match value {
            EventMask::MODIFY => Ok(FileEvent::Modified),
            EventMask::DELETE => Ok(FileEvent::Deleted),
            EventMask::CREATE => Ok(FileEvent::Created),
            _ => Err(NotifyStreamError::UnsupportedEventMask { mask: value }),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NotifyStreamError {
    #[error(transparent)]
    FromIOError(#[from] std::io::Error),

    #[error("Error creating event stream")]
    FailedToCreateStream,

    #[error("Error normalising path for: {path:?}")]
    FailedToNormalisePath { path: PathBuf },

    #[error("Unsupported mask: {mask:?}")]
    UnsupportedWatchMask { mask: WatchMask },

    #[error("Unsupported mask: {mask:?}")]
    UnsupportedEventMask { mask: EventMask },

    #[error("Expected watch directory to be: {expected:?} but was: {actual:?}")]
    WrongParentDirectory { expected: PathBuf, actual: PathBuf },

    #[error("Watcher: {mask} is duplicated for file: {path:?}")]
    DuplicateWatcher { mask: FileEvent, path: PathBuf },
}

#[derive(Debug, Default, Clone, Eq)]
pub struct EventDescription {
    dir_path: PathBuf,
    file_name: Option<String>,
    masks: HashSet<FileEvent>,
}

impl PartialEq for EventDescription {
    fn eq(&self, other: &Self) -> bool {
        self.dir_path == other.dir_path && self.file_name == other.file_name
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct WatchDescriptor {
    // When a file/directory is added to an inotify watcher, then it will return a watch descriptor (Wid).
    // The watcher can be added only for the existing files/directories. This is unique to a path given path.
    // When multiple files that do not exist in a directory are watched, then the watcher will be added to the parent directory.
    // So, the wid remains the same for all.   So, here vector of Event Description is maintained to cross-verify if the event
    // raised is for the registered file/directory and for the registered event masks.
    description: HashMap<c_int, Vec<EventDescription>>,
}

impl WatchDescriptor {
    #[cfg(test)]
    #[cfg(feature = "fs-notify")]
    pub fn get_event_description(&self) -> &HashMap<c_int, Vec<EventDescription>> {
        &self.description
    }

    #[cfg(test)]
    #[cfg(feature = "fs-notify")]
    /// get a set of `Masks` for a given `watch descriptor id`
    pub(super) fn get_mask_set_for_a_watch_descriptor(&mut self, wid: c_int) -> HashSet<FileEvent> {
        let fdvec = self.description.get(&wid).unwrap().to_owned();
        let mut masks = HashSet::new();
        for fod in fdvec {
            masks.extend(fod.masks);
        }
        masks
    }

    /// inserts new values in `self.watch_descriptor`. this takes care of inserting
    /// - Insert new description with `wid` as key and `EventDescription instance` as value
    /// - inserting or appending new masks
    fn insert(
        &mut self,
        wid: c_int,
        dir_path: PathBuf,
        file_name: Option<String>,
        masks: HashSet<FileEvent>,
    ) {
        let new_event_description = EventDescription {
            dir_path,
            file_name,
            masks,
        };

        let fd_vec = self.description.entry(wid).or_insert_with(Vec::new);
        for event_description in fd_vec.iter_mut() {
            if (*event_description).eq(&new_event_description) {
                // they are the same wrt dir_path and file_name, BUT the keys need to be updated
                new_event_description.masks.into_iter().for_each(|mask| {
                    event_description.masks.insert(mask);
                });
                return;
            }
        }
        // otherwise it was a new entry
        fd_vec.push(new_event_description);
    }
}

pub struct NotifyStream {
    buffer: [u8; 1024],
    inotify: Inotify,
    watchers: WatchDescriptor,
}

impl TryDefault for NotifyStream {
    type Error = NotifyStreamError;

    fn try_default() -> Result<Self, Self::Error> {
        let inotify = Inotify::init()?;
        let buffer = [0; 1024];

        Ok(Self {
            buffer,
            inotify,
            watchers: WatchDescriptor::default(),
        })
    }
}

/// to allow notify to watch for multiple events (CLOSE_WRITE, CREATE, MODIFY, etc...)
/// our internal enum `Masks` needs to be converted into a single `WatchMask` via bitwise OR
/// operations. (Note, our `Masks` type is an enum, `WatchMask` is a bitflag)
pub(crate) fn pipe_masks_into_watch_mask(masks: &[FileEvent]) -> WatchMask {
    let mut watch_mask = WatchMask::empty();
    for mask in masks {
        watch_mask |= (*mask).into()
    }
    watch_mask
}

impl NotifyStream {
    /// add a watcher to a file or to a directory
    ///
    /// this is implemented as a directory watcher regardless if a file is desired
    /// to be watched or if a directory. There is an internal data structure that
    /// keeps track of what is being watched - `self.watchers`
    /// The `stream` method determines whether the incoming event matches what is
    /// expected in `self.watchers`.
    ///
    /// # Watching directories
    ///
    /// ```rust
    /// use tedge_utils::fs_notify::{NotifyStream, FileEvent};
    /// use try_traits::default::TryDefault;
    /// use std::path::Path;
    ///
    /// let dir_path_a = Path::new("/tmp");
    /// let dir_path_b = Path::new("/etc");
    ///
    /// let mut fs_notification_stream = NotifyStream::try_default().unwrap();
    /// fs_notification_stream.add_watcher(dir_path_a, None, &[FileEvent::Created]).unwrap();
    /// fs_notification_stream.add_watcher(dir_path_b, None, &[FileEvent::Created, FileEvent::Deleted]).unwrap();
    /// ```
    ///
    /// # Watching files
    ///
    /// ```rust
    /// use tedge_utils::fs_notify::{NotifyStream, FileEvent};
    /// use tedge_test_utils::fs::TempTedgeDir;
    /// use try_traits::default::TryDefault;
    ///
    /// let ttd = TempTedgeDir::new();  // created a new tmp directory
    /// let file_a = ttd.file("file_a");
    /// let file_b = ttd.file("file_b");
    ///
    /// let mut fs_notification_stream = NotifyStream::try_default().unwrap();
    /// fs_notification_stream.add_watcher(ttd.path(), Some(String::from("file_a")), &[FileEvent::Modified]).unwrap();
    /// fs_notification_stream.add_watcher(ttd.path(), Some(String::from("file_b")), &[FileEvent::Created, FileEvent::Deleted]).unwrap();
    /// ```
    /// NOTE:
    /// in this last example, the root directory is the same: `ttd.path()`
    /// but the files watched and masks are different. In the background,
    /// the `add_watcher` fn will add a watch on `ttd.path()` with masks:
    /// Created, Modified and Deleted. and will update `self.watchers`
    /// with two entries, one for file_a and one for file_b.
    ///
    /// The `stream` method will check that events coming from
    /// `ttd.path()` match `self.watchers`
    pub fn add_watcher(
        &mut self,
        dir_path: &Path,
        file: Option<String>,
        events: &[FileEvent],
    ) -> Result<(), NotifyStreamError> {
        let watch_mask = pipe_masks_into_watch_mask(events);
        let wd = self.inotify.watches().add(dir_path, watch_mask)?;
        let masks = HashSet::from_iter(events.iter().copied());
        self.watchers.insert(
            wd.get_watch_descriptor_id(),
            dir_path.to_path_buf(),
            file,
            masks,
        );
        Ok(())
    }

    //// create an fs notification event stream
    pub fn stream(self) -> impl Stream<Item = Result<(PathBuf, FileEvent), NotifyStreamError>> {
        try_stream! {
            let mut notify_service = self.inotify.into_event_stream(self.buffer)?;
            while let Some(event_or_error) = notify_service.next().await {
                match event_or_error {
                    Ok(event) => {
                        let file_or_dir_vec = self.watchers.description.get(&event.wd.get_watch_descriptor_id());
                        if let Some(files) = file_or_dir_vec {
                           // let path = NotifyStream::get_full_path_and_mask(&event, files.to_vec())?;
                            if let Some(path) = NotifyStream::get_full_path_and_mask(&event, files.to_vec())? {
                                let mask: FileEvent = event.mask.try_into()?;
                                yield (Path::new(&path).to_path_buf(), mask)
                            }
                        }
                    }
                    Err(error) => {
                        // any error coming out of `notify_service.next()` will be
                        // an std::Io error: https://docs.rs/inotify/latest/src/inotify/stream.rs.html#48
                        yield Err(NotifyStreamError::FromIOError(error))?;
                    }
                }
            }
        }
    }

    fn get_full_path_and_mask(
        event: &Event<OsString>,
        files: Vec<EventDescription>,
    ) -> Result<Option<PathBuf>, NotifyStreamError> {
        // Unwrap is safe here because event will always contain a file/directory name.
        // The event is raised only on change in the directory. i.e either on create, modify or delete of a file/directory.
        let fname = event.name.as_ref().unwrap();
        // Check if file under watch. If so, then return the full path to the file and the event mask.
        for file in &files {
            if let Some(wfname) = file.file_name.as_ref() {
                if wfname.eq(&fname.to_string_lossy()) {
                    for mask in &file.masks {
                        if mask.eq(&event.mask.try_into()?) {
                            let mut full_path = file.dir_path.clone();
                            full_path.push(wfname);
                            return Ok(Some(full_path));
                        }
                    }
                }
            };
        }
        // All the files under this directory are watched. So, return full path to the file and the mask
        for dir in files {
            if dir.file_name.eq(&None) {
                for mask in &dir.masks {
                    if mask.eq(&event.mask.try_into()?) {
                        let mut full_path = dir.dir_path.clone();
                        full_path.push(fname);
                        return Ok(Some(full_path));
                    }
                }
            }
        }
        Ok(None)
    }
}

/// utility function to return an fs notify stream:
///
/// this supports both file wathes and directory watches:
///
/// # Example
/// ```rust
/// use tedge_utils::fs_notify::{fs_notify_stream, FileEvent};
/// use tedge_test_utils::fs::TempTedgeDir;
///
/// // created a new tmp directory with some files and directories
/// let ttd = TempTedgeDir::new();
/// let file_a = ttd.file("file_a");
/// let file_b = ttd.file("file_b");
/// let file_c = ttd.dir("some_directory").file("file_c");
///
///
/// let fs_notification_stream = fs_notify_stream(&[
///      (ttd.path(), Some(String::from("file_a")), &[FileEvent::Created]),
///      (ttd.path(), Some(String::from("file_b")), &[FileEvent::Modified, FileEvent::Created]),
///      (ttd.path(), Some(String::from("some_directory/file_c")), &[FileEvent::Deleted])
///     ]
/// ).unwrap();
/// ```
pub fn fs_notify_stream(
    input: &[(&Path, Option<String>, &[FileEvent])],
) -> Result<impl Stream<Item = Result<(PathBuf, FileEvent), NotifyStreamError>>, NotifyStreamError>
{
    let mut fs_notification_service = NotifyStream::try_default()?;
    for (dir_path, watch, flags) in input {
        fs_notification_service.add_watcher(dir_path, watch.to_owned(), flags)?;
    }
    Ok(fs_notification_service.stream())
}

