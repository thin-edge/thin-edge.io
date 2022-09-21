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

    #[error("Wrong Event triggered")]
    WrongEventTriggered,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EventDescription {
    pub dir_path: PathBuf,
    pub file_name: Option<String>,
    pub masks: HashSet<FileEvent>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct WatchDescriptor {
    description: HashMap<c_int, Vec<EventDescription>>,
}

impl WatchDescriptor {
    #[cfg(test)]
    #[cfg(feature = "fs-notify")]
    pub fn get_watch_descriptor(&self) -> &HashMap<c_int, Vec<EventDescription>> {
        &self.description
    }

    #[cfg(test)]
    #[cfg(feature = "fs-notify")]
    /// get a set of `Masks` for a given `watch descriptor id`
    pub(super) fn get_mask_set_for_a_watch_discriptor(&mut self, wid: c_int) -> HashSet<FileEvent> {
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
    /// NOTE: though it is not a major concern, the `masks` entry is unordered
    /// vec![Masks::Deleted, Masks::Modified] does not equal vec![Masks::Modified, Masks::Deleted]
    fn insert(
        &mut self,
        wid: c_int,
        dir_path: PathBuf,
        file_name: Option<String>,
        masks: HashSet<FileEvent>,
    ) {
        let file_or_dir = EventDescription {
            dir_path,
            file_name,
            masks,
        };

        let fd_vec = match self.description.get(&wid) {
            Some(v) => {
                let mut fvec = v.to_owned();
                fvec.push(file_or_dir);
                fvec
            }
            None => {
                vec![file_or_dir]
            }
        };
        self.description.insert(wid, fd_vec);
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
                            let (path, mask) = NotifyStream::get_full_path_and_mask(&event, files.to_vec())?;
                            yield (Path::new(&path).to_path_buf(), mask)
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
    ) -> Result<(String, FileEvent), NotifyStreamError> {
        // Unwrap is safe here because event will always contain a file name.
        let fname = event.name.as_ref().unwrap();
        // Check if file under watch. If so, then return the full path to the file and the event mask.
        for file in &files {
            if let Some(wfname) = file.file_name.as_ref() {
                if wfname.eq(&fname.to_string_lossy()) {
                    for mask in &file.masks {
                        if mask.eq(&event.mask.try_into()?) {
                            let full_path = format!("{}/{wfname}", file.dir_path.to_string_lossy());
                            return Ok((full_path, mask.to_owned()));
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
                        let full_path = format!(
                            "{}/{}",
                            dir.dir_path.to_string_lossy(),
                            fname.to_string_lossy()
                        );
                        return Ok((full_path, mask.to_owned()));
                    }
                }
            }
        }
        Err(NotifyStreamError::WrongEventTriggered)
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

#[cfg(test)]
#[cfg(feature = "fs-notify")]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
        sync::Arc,
    };

    use futures::{pin_mut, Stream, StreamExt};

    use maplit::hashmap;
    use tedge_test_utils::fs::TempTedgeDir;
    use try_traits::default::TryDefault;

    use crate::fs_notify::FileEvent;

    use super::{
        fs_notify_stream, EventDescription, NotifyStream, NotifyStreamError, WatchDescriptor,
    };

    #[test]
    /// this test checks the underlying data structure `WatchDescriptor.description`
    /// three files are created:
    /// - file_a, at root level of `TempTedgeDir`
    /// - file_c, at level: `TempTedgeDir`/new_dir
    fn test_watch_descriptor_data_field() {
        let ttd = TempTedgeDir::new();
        let new_dir = ttd.dir("new_dir");

        let expected_data_structure = hashmap! {
            2 => vec![EventDescription{dir_path:new_dir.to_path_buf(),file_name:Some("file_c".to_string()),masks:HashSet::from([FileEvent::Created, FileEvent::Modified]),}],
            1 => vec![EventDescription{dir_path:ttd.to_path_buf(),file_name:Some("file_a".to_string()),masks:HashSet::from([FileEvent::Created,FileEvent::Modified, FileEvent::Deleted]),}],
        };
        let expected_hash_set_for_root_dir =
            HashSet::from([FileEvent::Created, FileEvent::Modified, FileEvent::Deleted]);
        let expected_hash_set_for_new_dir =
            HashSet::from([FileEvent::Created, FileEvent::Modified]);

        let mut actual_data_structure = WatchDescriptor::default();
        actual_data_structure.insert(
            1,
            ttd.path().to_path_buf(),
            Some(String::from("file_a")),
            HashSet::from([FileEvent::Created, FileEvent::Modified, FileEvent::Deleted]),
        );

        actual_data_structure.insert(
            2,
            new_dir.path().to_path_buf(),
            Some(String::from("file_c")),
            HashSet::from([FileEvent::Created, FileEvent::Modified]),
        );

        assert!(actual_data_structure
            .get_watch_descriptor()
            .eq(&expected_data_structure));

        assert_eq!(
            actual_data_structure.get_mask_set_for_a_watch_discriptor(1),
            expected_hash_set_for_root_dir
        );

        assert_eq!(
            actual_data_structure.get_mask_set_for_a_watch_discriptor(2),
            expected_hash_set_for_new_dir
        );
    }

    #[test]
    fn test_add_watcher() {
        let ttd = TempTedgeDir::new();
        let new_dir = ttd.dir("new_dir");
        ttd.file("file_a");
        ttd.file("file_b");
        new_dir.file("file_c");

        let mut notify_service = NotifyStream::try_default().unwrap();
        notify_service
            .add_watcher(
                ttd.path(),
                Some(String::from("file_a")),
                &[FileEvent::Created],
            )
            .unwrap();
        notify_service
            .add_watcher(
                ttd.path(),
                Some(String::from("file_a")),
                &[FileEvent::Created, FileEvent::Deleted],
            )
            .unwrap();
        notify_service
            .add_watcher(
                ttd.path(),
                Some(String::from("file_b")),
                &[FileEvent::Modified],
            )
            .unwrap();
        notify_service
            .add_watcher(
                new_dir.path(),
                Some(String::from("file_c")),
                &[FileEvent::Deleted],
            )
            .unwrap();
    }

    async fn assert_stream(
        mut inputs: HashMap<String, Vec<FileEvent>>,
        stream: Result<
            impl Stream<Item = Result<(PathBuf, FileEvent), NotifyStreamError>>,
            NotifyStreamError,
        >,
    ) {
        let stream = stream.unwrap();
        pin_mut!(stream);
        while let Some(Ok((path, flag))) = stream.next().await {
            let file_name = String::from(path.file_name().unwrap().to_str().unwrap());
            let mut values = match inputs.get_mut(&file_name) {
                Some(v) => v.to_vec(),
                None => {
                    inputs.remove(&file_name);
                    continue;
                }
            };
            match values.iter().position(|x| *x == flag) {
                Some(i) => values.remove(i),
                None => {
                    continue;
                }
            };
            if values.is_empty() {
                inputs.remove(&file_name);
            } else {
                inputs.insert(file_name, values);
            }
            if inputs.is_empty() {
                break;
            }
        }
    }

    #[tokio::test]
    async fn test_multiple_known_files_watched() {
        let ttd = Arc::new(TempTedgeDir::new());
        let ttd_clone = ttd.clone();

        let expected_events = hashmap! {
            String::from("file_a") => vec![FileEvent::Created],
            String::from("file_b") => vec![FileEvent::Created, FileEvent::Modified]
        };

        let stream = fs_notify_stream(&[
            (
                ttd.path(),
                Some(String::from("file_a")),
                &[FileEvent::Created],
            ),
            (
                ttd.path(),
                Some(String::from("file_b")),
                &[FileEvent::Created, FileEvent::Modified],
            ),
        ]);

        let fs_notify_handler = tokio::task::spawn(async move {
            assert_stream(expected_events, stream).await;
        });

        let file_handler = tokio::task::spawn(async move {
            ttd_clone.file("file_a").with_raw_content("content");
            ttd_clone.file("file_b").with_raw_content("content");
        });

        fs_notify_handler.await.unwrap();
        file_handler.await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_unknown_files_watched() {
        let ttd = Arc::new(TempTedgeDir::new());
        ttd.file("file_b"); // creating this file before the fs notify service
        let ttd_clone = ttd.clone();

        let expected_events = hashmap! {
            String::from("file_a") => vec![FileEvent::Created],
            String::from("file_b") => vec![FileEvent::Modified],
            String::from("file_c") => vec![FileEvent::Created, FileEvent::Deleted]
        };

        let stream = fs_notify_stream(&[(
            ttd.path(),
            None,
            &[FileEvent::Created, FileEvent::Modified, FileEvent::Deleted],
        )]);

        let fs_notify_handler = tokio::task::spawn(async move {
            assert_stream(expected_events, stream).await;
        });

        let file_handler = tokio::task::spawn(async move {
            ttd_clone.file("file_a"); // should match CREATE
            ttd_clone.file("file_b").with_raw_content("content"); // should match MODIFY
            ttd_clone.file("file_c").delete(); // should match CREATE, DELETE
        });

        fs_notify_handler.await.unwrap();
        file_handler.await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_directories_watched() {
        let ttd_a = Arc::new(TempTedgeDir::new());
        let ttd_b = Arc::new(TempTedgeDir::new());
        let ttd_c = Arc::new(TempTedgeDir::new());
        let ttd_d = Arc::new(TempTedgeDir::new());

        let ttd_a_clone = ttd_a.clone();
        let ttd_b_clone = ttd_b.clone();
        let ttd_c_clone = ttd_c.clone();
        let ttd_d_clone = ttd_d.clone();

        let expected_events = hashmap! {
            String::from("file_a") => vec![FileEvent::Created],
            String::from("file_b") => vec![FileEvent::Created, FileEvent::Modified],
            String::from("file_c") => vec![FileEvent::Created, FileEvent::Deleted],
            String::from("dir_d") => vec![FileEvent::Created],
        };

        let stream = fs_notify_stream(&[
            (ttd_a.path(), None, &[FileEvent::Created]),
            (
                ttd_b.path(),
                None,
                &[FileEvent::Created, FileEvent::Modified],
            ),
            (
                ttd_c.path(),
                None,
                &[FileEvent::Created, FileEvent::Deleted],
            ),
            (ttd_d.path(), None, &[FileEvent::Created]),
        ]);

        let fs_notify_handler = tokio::task::spawn(async move {
            assert_stream(expected_events, stream).await;
        });

        let file_handler = tokio::task::spawn(async move {
            ttd_a_clone.file("file_a"); // should match CREATE
            ttd_b_clone.file("file_b").with_raw_content("content"); // should match MODIFY;
            ttd_c_clone.file("file_c").delete(); // should match CREATE, DELETE file;
            ttd_d_clone.dir("dir_d"); // should match CREATE directory;
        });

        fs_notify_handler.await.unwrap();
        file_handler.await.unwrap();
    }
}
