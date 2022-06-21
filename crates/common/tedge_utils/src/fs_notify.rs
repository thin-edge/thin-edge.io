use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    path::{Path, PathBuf},
};

use async_stream::try_stream;
pub use futures::{pin_mut, Stream, StreamExt};
use inotify::{EventMask, Inotify, WatchMask};
use strum_macros::Display;
use try_traits::default::TryDefault;

#[derive(Debug, Display, PartialEq, Eq, Clone, Hash)]
pub enum Masks {
    Modified,
    Deleted,
    Created,
    Undefined,
}

impl From<Masks> for WatchMask {
    fn from(masks: Masks) -> Self {
        match masks {
            Masks::Modified => WatchMask::MODIFY,
            Masks::Deleted => WatchMask::DELETE,
            Masks::Created => WatchMask::CREATE,
            Masks::Undefined => WatchMask::empty(),
        }
    }
}

impl From<EventMask> for Masks {
    fn from(em: EventMask) -> Self {
        match em {
            EventMask::MODIFY => Masks::Modified,
            EventMask::DELETE => Masks::Deleted,
            EventMask::CREATE => Masks::Created,
            _ => Masks::Undefined,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NotifyStreamError {
    #[error(transparent)]
    FromIOError(#[from] std::io::Error),

    #[error("Error starting fs notification service.")]
    NotifyInitError,

    #[error("Error creating event stream")]
    ErrorCreatingStream,

    #[error("Error normalising watcher for: {path:?}")]
    ErrorNormalisingWatcher { path: PathBuf },

    // FIXME: how to show which mask is unsupported when inotify::WatchMask
    // does not impl Display?
    #[error("Unsupported mask.")]
    UnsupportedWatchMask,

    #[error("Expected watch directory to be: {expected:?} but was: {actual:?}")]
    WrongParentDirectory { expected: PathBuf, actual: PathBuf },
}

#[derive(Debug, Default, Clone)]
struct WatchDescriptor {
    pub dir_path: PathBuf,
    pub key: HashMap<String, Vec<Masks>>,
}

impl Eq for WatchDescriptor {}

impl PartialEq for WatchDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.dir_path == other.dir_path && self.key == other.key
    }
}

impl Hash for WatchDescriptor {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.dir_path.hash(state);
        for key in self.key.keys() {
            key.hash(state);
        }
    }
}

impl WatchDescriptor {
    fn new(dir_path: PathBuf, file_name: String, masks: Vec<Masks>) -> Self {
        let mut hm = HashMap::new();
        hm.insert(file_name, masks);
        Self { dir_path, key: hm }
    }
}

pub struct NotifyStream {
    buffer: [u8; 1024],
    inotify: Inotify,
    watchers: Option<HashSet<WatchDescriptor>>,
}

impl TryDefault for NotifyStream {
    type Error = NotifyStreamError;

    fn try_default() -> Result<Self, Self::Error> {
        let inotify = Inotify::init();
        match inotify {
            Ok(inotify) => {
                let buffer = [0; 1024];
                Ok(Self {
                    buffer,
                    inotify,
                    watchers: None,
                })
            }
            Err(err) => Err(NotifyStreamError::FromIOError(err)),
        }
    }
}

/// normalisation step joining `candidate_watch_dir` and `candidate_file` and computing the parent of `candidate_file`.
///
/// this is useful in situations where:
/// `candidate_watch_dir` = /path/to/a/directory
/// `candidate_file` = continued/path/to/a/file
///
/// this function will concatenate the two, into:
/// `/path/to/a/directory/continued/path/to/a/file`
/// and will return:
/// `/path/to/a/directory/continued/path/to/a/` and `file`
fn normalising_watch_dir_and_file(
    candidate_watch_dir: &Path,
    candidate_file: &str,
) -> Result<(PathBuf, String), NotifyStreamError> {
    let full_path = candidate_watch_dir.join(candidate_file);
    let full_path = &full_path;
    let parent = full_path
        .parent()
        .ok_or_else(|| NotifyStreamError::ErrorNormalisingWatcher {
            path: full_path.to_path_buf(),
        })?;
    let file = full_path
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or_else(|| NotifyStreamError::ErrorNormalisingWatcher {
            path: full_path.to_path_buf(),
        })?;

    Ok((parent.to_path_buf(), file.to_string()))
}

/// to allow notify to watch for multiple events (CLOSE_WRITE, CREATE, MODIFY, etc...)
/// our internal enum `Masks` needs to be converted into a single `WatchMask` via bitwise OR
/// operations. (Note, our `Masks` type is an enum, `WatchMask` is a bitflag)
pub(crate) fn pipe_masks_into_watch_mask(masks: &[Masks]) -> WatchMask {
    let mut watch_mask = WatchMask::empty();
    for mask in masks {
        watch_mask |= mask.clone().into()
    }
    watch_mask
}

impl NotifyStream {
    /// add a watcher to a file or to a directory
    ///
    /// this is implemeted as a direcotry watcher regardless if a file is desired
    /// to be watched or if a directory. There is an internal data structure that
    /// keeps track of what is being watched - `self.watchers`
    /// The `stream` method determines whether the incoming event matches what is
    /// expected in `self.watchers`.
    ///
    /// # Watching directories
    ///
    /// ```rust
    /// use tedge_utils::fs_notify::{NotifyStream, Masks};
    /// use try_traits::default::TryDefault;
    /// use std::path::Path;
    ///
    /// let dir_path_a = Path::new("/tmp");
    /// let dir_path_b = Path::new("/etc/tedge/c8y");
    ///
    /// let mut fs_notification_stream = NotifyStream::try_default().unwrap();
    /// fs_notification_stream.add_watcher(dir_path_a, String::from("*"), &[Masks::Created]).unwrap();
    /// fs_notification_stream.add_watcher(dir_path_b, String::from("*"), &[Masks::Created, Masks::Deleted]).unwrap();
    /// ```
    ///
    /// # Watching files
    ///
    /// ```rust
    /// use tedge_utils::fs_notify::{NotifyStream, Masks};
    /// use tedge_test_utils::fs::TempTedgeDir;
    /// use try_traits::default::TryDefault;
    ///
    /// let ttd = TempTedgeDir::new();  // created a new tmp directory
    /// let file_a = ttd.file("file_a");
    /// let file_b = ttd.file("file_b");
    ///
    /// let mut fs_notification_stream = NotifyStream::try_default().unwrap();
    /// fs_notification_stream.add_watcher(ttd.path(), String::from("file_a"), &[Masks::Modified]).unwrap();
    /// fs_notification_stream.add_watcher(ttd.path(), String::from("file_b"), &[Masks::Created, Masks::Deleted]).unwrap();
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
        file: String,
        masks: &[Masks],
    ) -> Result<(), NotifyStreamError> {
        // the first step if to normalise `dir_path` and `file`
        // this step is done just in case `file` is not just the name
        // but is a partial path to the file. For more info see the
        // function's docstring.
        let (dir_path, file) = normalising_watch_dir_and_file(dir_path, &file)?;
        let dir_path = dir_path.as_path();

        if let Some(mut watcher) = self.watchers.clone() {
            let mut all_flags: HashSet<_> = watcher
                .iter()
                .flat_map(|set| set.key.values())
                .flatten()
                .collect();
            for mask in masks {
                all_flags.insert(mask);
            }

            let watch_mask = pipe_masks_into_watch_mask(masks);
            let _ = self.inotify.add_watch(dir_path, watch_mask);
            let wd = WatchDescriptor::new(dir_path.to_path_buf(), file, masks.to_vec());
            watcher.insert(wd);
            self.watchers = Some(watcher);
        } else {
            let watch_mask = pipe_masks_into_watch_mask(masks);
            let _ = self.inotify.add_watch(dir_path, watch_mask);
            let wd = WatchDescriptor::new(dir_path.to_path_buf(), file, masks.to_vec());
            let mut hs = HashSet::new();
            hs.insert(wd);
            self.watchers = Some(hs);
        }
        Ok(())
    }

    /// create an fs notification event stream
    pub fn stream(mut self) -> impl Stream<Item = Result<(PathBuf, Masks), NotifyStreamError>> {
        try_stream! {
            let mut notify_service = self.inotify.event_stream(self.buffer)?;
            while let Some(event_or_error) = notify_service.next().await {
                match event_or_error {
                    Ok(event) => {
                        let event_mask: Masks = event.mask.into();
                        // because watching a file or watching a direcotry is implemented as
                        // watching a directory, we can ignore the case where &event.name is None
                        if let Some(event_name) = &event.name {
                            let notify_file_name = event_name.to_str().ok_or_else(|| NotifyStreamError::ErrorCreatingStream)?;
                            // inotify triggered for a file named `notify_file_name`. Next we need
                            // to see if we have a matching entry WITH a matching flag/mask in `self.watchers`
                            for watcher in self.watchers.as_ref().ok_or_else(|| NotifyStreamError::ErrorCreatingStream)? {
                                for (file_name, flags) in &watcher.key {
                                    for flag in flags {
                                        // There are two cases:
                                        // 1. we added a file watch
                                        // 2. we added a directory watch
                                        //
                                        // for case 1. our input could have been something like:
                                        // ...
                                        // notify_service.add_watcher(
                                        //          "/path/to/some/place",
                                        //          "file_name",    <------ note file name is given
                                        //          &[Masks::Created]
                                        //  )
                                        // here the file we are watching is *given* - so we can yield events with the
                                        // corresponding `event_name` and mask.
                                        if file_name.eq(notify_file_name) && event_mask.clone().eq(flag) {
                                            let full_path = watcher.dir_path.join(file_name.clone());
                                            yield (full_path, event_mask.clone())
                                        // for case 2. our input could have been something like:
                                        // notify_service.add_watcher(
                                        //          "/path/to/some/place",
                                        //          "*",            <------ note the file name is not given
                                        //          &[Masks::Created]
                                        //  )
                                        // here the file we are watching is not known to us, so we match only on event mask
                                        } else if file_name.eq("*")  && event_mask.clone().eq(flag) {
                                            let full_path = watcher.dir_path.join(notify_file_name);
                                            yield (full_path, event_mask.clone())
                                        }
                                    }
                                }
                            }
                        }
                    },
                    Err(error) => {
                        // any error comming out of `notify_service.next()` will be
                        // an std::Io error: https://docs.rs/inotify/latest/src/inotify/stream.rs.html#48
                        yield Err(NotifyStreamError::FromIOError(error))?;
                    }
                }
            }
        }
    }
}

/// utility function to return an fs notify stream:
///
/// this supports both file wathes and directory watches:
///
/// # Example
/// ```rust
/// use tedge_utils::fs_notify::{fs_notify_stream, Masks};
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
///      (ttd.path(), String::from("file_a"), &[Masks::Created]),
///      (ttd.path(), String::from("file_b"), &[Masks::Modified, Masks::Created]),
///      (ttd.path(), String::from("some_directory/file_c"), &[Masks::Deleted])
///     ]
/// ).unwrap();
/// ```
pub fn fs_notify_stream(
    input: &[(&Path, String, &[Masks])],
) -> Result<impl Stream<Item = Result<(PathBuf, Masks), NotifyStreamError>>, NotifyStreamError> {
    let mut fs_notification_service = NotifyStream::try_default()?;
    for (dir_path, file_name, flags) in input {
        fs_notification_service.add_watcher(dir_path, file_name.to_owned(), flags)?;
    }
    Ok(fs_notification_service.stream())
}
#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf, sync::Arc};

    use super::{fs_notify_stream, pin_mut, Masks, NotifyStreamError, Stream, StreamExt};
    use maplit::hashmap;
    use tedge_test_utils::fs::TempTedgeDir;

    async fn assert_stream(
        mut inputs: HashMap<String, Vec<Masks>>,
        stream: Result<
            impl Stream<Item = Result<(PathBuf, Masks), NotifyStreamError>>,
            NotifyStreamError,
        >,
    ) {
        let stream = stream.unwrap();
        pin_mut!(stream);
        while let Some(Ok((path, flag))) = stream.next().await {
            let file_name = String::from(path.file_name().unwrap().to_str().unwrap());
            let mut values = inputs.get_mut(&file_name).unwrap().to_vec();
            let index = values.iter().position(|x| *x == flag).unwrap();
            values.remove(index);

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
            String::from("file_a") => vec![Masks::Created],
            String::from("file_b") => vec![Masks::Created, Masks::Modified]
        };

        let stream = fs_notify_stream(&[
            (ttd.path(), String::from("file_a"), &[Masks::Created]),
            (
                ttd.path(),
                String::from("file_b"),
                &[Masks::Created, Masks::Modified],
            ),
        ]);

        let fs_notify_handler = tokio::task::spawn(async move {
            assert_stream(expected_events, stream).await;
        });

        let file_handler = tokio::task::spawn(async move {
            ttd_clone.file("file_a").with_raw_content("content");
            ttd_clone.file("file_b").with_raw_content("content");
        });

        let () = fs_notify_handler.await.unwrap();
        let () = file_handler.await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_unknown_files_watched() {
        let ttd = Arc::new(TempTedgeDir::new());
        ttd.file("file_b"); // creating this file before the fs notify service
        let ttd_clone = ttd.clone();

        let expected_events = hashmap! {
            String::from("file_a") => vec![Masks::Created],
            String::from("file_b") => vec![Masks::Modified],
            String::from("file_c") => vec![Masks::Created, Masks::Deleted]
        };

        let stream = fs_notify_stream(&[(
            ttd.path(),
            String::from("*"),
            &[Masks::Created, Masks::Modified, Masks::Deleted],
        )]);

        let fs_notify_handler = tokio::task::spawn(async move {
            assert_stream(expected_events, stream).await;
        });

        let file_handler = tokio::task::spawn(async move {
            ttd_clone.file("file_a"); // should match CREATE
            ttd_clone.file("file_b").with_raw_content("content"); // should match MODIFY
            ttd_clone.file("file_c").delete(); // should match CREATE, DELETE
        });

        let () = fs_notify_handler.await.unwrap();
        let () = file_handler.await.unwrap();
    }
}
