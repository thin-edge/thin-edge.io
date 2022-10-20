use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use notify::{
    event::{AccessKind, AccessMode, CreateKind, DataChange, ModifyKind, RemoveKind},
    Config, EventKind, INotifyWatcher, RecommendedWatcher, RecursiveMode, Watcher,
};
use tokio::sync::mpsc::{channel, Receiver};
use try_traits::default::TryDefault;

use strum_macros::Display;

#[derive(Debug, Display, PartialEq, Eq, Clone, Hash, PartialOrd, Ord, Copy)]
pub enum FileEvent {
    Modified,
    Deleted,
    Created,
}

#[derive(Debug, thiserror::Error)]
pub enum NotifyStreamError {
    #[error(transparent)]
    FromIOError(#[from] std::io::Error),

    #[error(transparent)]
    FromNotifyError(#[from] notify::Error),

    #[error("Error creating event stream")]
    FailedToCreateStream,

    #[error("Error normalising path for: {path:?}")]
    FailedToNormalisePath { path: PathBuf },

    #[error("Expected watch directory to be: {expected:?} but was: {actual:?}")]
    WrongParentDirectory { expected: PathBuf, actual: PathBuf },

    #[error("Watcher: {mask} is duplicated for file: {path:?}")]
    DuplicateWatcher { mask: FileEvent, path: PathBuf },
}

type DirPath = PathBuf;
type MaybeFileName = Option<String>;
type Metadata = HashMap<DirPath, HashMap<MaybeFileName, HashSet<FileEvent>>>;

pub struct NotifyStream {
    watcher: INotifyWatcher,
    pub rx: Receiver<(PathBuf, FileEvent)>,
    metadata: Metadata,
}

impl TryDefault for NotifyStream {
    type Error = NotifyStreamError;

    fn try_default() -> Result<Self, Self::Error> {
        let (tx, rx) = channel(1024);

        // Automatically select the best implementation for your platform.
        // You can also access each implementation directly e.g. INotifyWatcher.
        let watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                futures::executor::block_on(async {
                    if let Ok(notify_event) = res {
                        match notify_event.kind {
                            EventKind::Modify(ModifyKind::Data(DataChange::Any)) => {
                                for path in notify_event.paths {
                                    let _ = tx.send((path, FileEvent::Modified)).await;
                                }
                            }

                            EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
                                for path in notify_event.paths {
                                    let _ = tx.send((path, FileEvent::Modified)).await;
                                }
                            }
                            EventKind::Create(CreateKind::File) => {
                                for path in notify_event.paths {
                                    let _ = tx.send((path, FileEvent::Created)).await;
                                }
                            }
                            EventKind::Create(CreateKind::Folder) => {
                                for path in notify_event.paths {
                                    let _ = tx.send((path, FileEvent::Created)).await;
                                }
                            }
                            EventKind::Remove(RemoveKind::File) => {
                                for path in notify_event.paths {
                                    let _ = tx.send((path, FileEvent::Deleted)).await;
                                }
                            }
                            EventKind::Remove(RemoveKind::Folder) => {
                                for path in notify_event.paths {
                                    let _ = tx.send((path, FileEvent::Deleted)).await;
                                }
                            }
                            _other => {}
                        }
                    }
                })
            },
            Config::default(),
        )?;
        Ok(Self {
            watcher,
            rx,
            metadata: HashMap::new(),
        })
    }
}

impl NotifyStream {
    #[cfg(test)]
    fn get_metadata(&self) -> &Metadata {
        &self.metadata
    }
    fn get_metadata_as_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }

    fn add_watcher(
        &mut self,
        dir_path: &Path,
        file: Option<String>,
        events: &[FileEvent],
    ) -> Result<(), NotifyStreamError> {
        self.watcher.watch(dir_path, RecursiveMode::Recursive)?;

        // we add the information to the metadata
        let maybe_file_name_entry = self
            .get_metadata_as_mut()
            .entry(dir_path.to_path_buf())
            .or_insert_with(HashMap::new);

        let file_event_entry = maybe_file_name_entry
            .entry(file)
            .or_insert_with(HashSet::new);
        for event in events {
            file_event_entry.insert(*event);
        }
        Ok(())
    }
}

pub fn fs_notify_stream(
    input: &[(&Path, Option<String>, &[FileEvent])],
) -> Result<NotifyStream, NotifyStreamError> {
    let mut fs_notify = NotifyStream::try_default()?;
    for (dir_path, watch, flags) in input {
        fs_notify.add_watcher(dir_path, watch.to_owned(), flags)?;
    }
    Ok(fs_notify)
}

#[cfg(test)]
mod notify_tests {
    use std::{collections::HashMap, sync::Arc};

    use maplit::hashmap;
    use tedge_test_utils::fs::TempTedgeDir;
    use try_traits::default::TryDefault;

    use crate::fs_notify::{FileEvent, NotifyStreamError};

    use super::{fs_notify_stream, NotifyStream};

    /// This test:
    ///     Creates a duplicate watcher (same directory path, same file name, same event)
    ///     Adds a new event for the same directory path, same file name
    ///     Checks the duplicate event is not duplicated in the data structure
    ///     Checks the new event is in the data structure
    #[test]
    fn it_inserts_new_file_events_correctly() {
        let ttd = TempTedgeDir::new();
        let mut notify = NotifyStream::try_default().unwrap();
        let maybe_file_name = Some("file_a".to_string());

        notify
            .add_watcher(ttd.path(), maybe_file_name.clone(), &[FileEvent::Created])
            .unwrap();
        notify
            .add_watcher(ttd.path(), maybe_file_name.clone(), &[FileEvent::Created])
            .unwrap();
        notify
            .add_watcher(ttd.path(), maybe_file_name.clone(), &[FileEvent::Deleted])
            .unwrap();

        let event_hashset = notify
            .get_metadata()
            .get(ttd.path())
            .unwrap()
            .get(&maybe_file_name)
            .unwrap();

        // assert no duplicate entry was created for the second insert and new event was added
        // in total 2 events are expected: FileEvent::Created, FileEvent::Deleted
        assert_eq!(event_hashset.len(), 2);
        assert!(event_hashset.contains(&FileEvent::Created));
        assert!(event_hashset.contains(&FileEvent::Deleted));
    }

    async fn assert_rx_stream(
        mut inputs: HashMap<String, Vec<FileEvent>>,
        mut fs_notify: NotifyStream,
    ) {
        while let Some((path, flag)) = fs_notify.rx.recv().await {
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
        ])
        .unwrap();

        let fs_notify_handler = tokio::task::spawn(async move {
            assert_rx_stream(expected_events, stream).await;
        });

        let file_handler = tokio::task::spawn(async move {
            ttd_clone.file("file_a").with_raw_content("content");
            ttd_clone.file("file_b").with_raw_content("content");
        });

        fs_notify_handler.await.unwrap();
        file_handler.await.unwrap();
    }

    #[tokio::test]
    async fn it_works() {
        let ttd = Arc::new(TempTedgeDir::new());
        let ttd_clone = ttd.clone();
        let mut fs_notify = NotifyStream::try_default().unwrap();
        fs_notify
            .add_watcher(ttd.path(), None, &[FileEvent::Created, FileEvent::Modified])
            .unwrap();

        let expected_events = hashmap! {
            String::from("file_a") => vec![FileEvent::Created],
            String::from("file_b") => vec![FileEvent::Created, FileEvent::Modified],
            String::from("file_c") => vec![FileEvent::Created, FileEvent::Deleted],
        };

        let file_system_handler = tokio::spawn(async move {
            ttd_clone.dir("dir_a");
            ttd_clone.file("file_a");
            ttd_clone.file("file_b"); //.with_raw_content("yo");
            ttd_clone.file("file_c").delete();
        });

        let fs_notify_handler = tokio::spawn(async move {
            assert_rx_stream(expected_events, fs_notify).await;
        });

        fs_notify_handler.await.unwrap();
        file_system_handler.await.unwrap();
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
        )])
        .unwrap();

        let fs_notify_handler = tokio::task::spawn(async move {
            assert_rx_stream(expected_events, stream).await;
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
        ])
        .unwrap();

        let fs_notify_handler = tokio::task::spawn(async move {
            assert_rx_stream(expected_events, stream).await;
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
