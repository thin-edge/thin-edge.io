use debouncer::DebouncedEvent;
use notify::event::AccessKind;
use notify::event::AccessMode;
use notify::event::CreateKind;
use notify::event::RemoveKind;
use notify::EventKind;
use notify::RecommendedWatcher;
use notify::RecursiveMode;
use notify_debouncer_full as debouncer;
use std::hash::Hash;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use strum::Display;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::Receiver;

/// The time window in which fs events will be subject to debouncing.
const NOTIFY_STREAM_DEBOUNCE_DURATION: Duration = Duration::from_millis(50);

/// The type of filesystem event that happened.
///
/// It needs to be noted that filesystem notifications can be unreliable, and different editors can use different
/// operations to update the file[1]. Additionally, we support different kinds of filesystem events, but often consumers
/// want to react only once to when the file is updated, so they're forced between subscribing to everything and
/// repeating work unnecessarily, or only to some and potentially missing other events. For our purposes, we always emit
/// [`FsEvent::Modified`] if the content of the file changed for any reason, i.e. due to rename, delete, create, modify,
/// etc.
///
/// [1]: https://github.com/notify-rs/notify/issues/113#issuecomment-281836995
#[derive(Debug, Display, PartialEq, Eq, Clone, Hash, PartialOrd, Ord, Copy)]
pub enum FsEvent {
    /// Returned when the content of the file changes for any reason.
    ///
    ///
    /// Emitted when the file is modified, deleted, created, or renamed. Consumers interested in responding to changes
    /// only once, without precise knowledge what caused the change of the content of the file, should use this event.
    Modified,
    FileDeleted,
    FileCreated,
    DirectoryDeleted,
    DirectoryCreated,
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
    DuplicateWatcher { mask: FsEvent, path: PathBuf },
}

pub struct NotifyStream {
    debouncer: debouncer::Debouncer<RecommendedWatcher, debouncer::NoCache>,
    pub rx: Receiver<(PathBuf, FsEvent)>,
}

impl NotifyStream {
    pub fn try_default() -> Result<Self, NotifyStreamError> {
        let (tx, rx) = channel(1024);

        // Automatically select the best implementation for your platform.
        // You can also access each implementation directly e.g. INotifyWatcher.
        let watcher = debouncer::new_debouncer_opt(
            NOTIFY_STREAM_DEBOUNCE_DURATION,
            None,
            move |res: Result<Vec<DebouncedEvent>, Vec<notify::Error>>| {
                futures::executor::block_on(async {
                    let Ok(debounced_events) = res else {
                        return;
                    };

                    // simplify event type
                    let mut debounced_events: Vec<_> = debounced_events
                        .into_iter()
                        .filter_map(|notify_event| {
                            let event = match notify_event.kind {
                                EventKind::Access(access_kind) => match access_kind {
                                    AccessKind::Close(access_mode) => match access_mode {
                                        AccessMode::Any | AccessMode::Other | AccessMode::Write => {
                                            Some(FsEvent::Modified)
                                        }
                                        AccessMode::Read | AccessMode::Execute => None,
                                    },
                                    AccessKind::Any | AccessKind::Other => Some(FsEvent::Modified),
                                    AccessKind::Read | AccessKind::Open(_) => None,
                                },
                                EventKind::Create(create_kind) => match create_kind {
                                    CreateKind::File | CreateKind::Any | CreateKind::Other => {
                                        Some(FsEvent::FileCreated)
                                    }
                                    CreateKind::Folder => Some(FsEvent::DirectoryCreated),
                                },
                                EventKind::Modify(_) => Some(FsEvent::Modified),
                                EventKind::Remove(remove_kind) => match remove_kind {
                                    RemoveKind::File | RemoveKind::Any | RemoveKind::Other => {
                                        Some(FsEvent::FileDeleted)
                                    }
                                    RemoveKind::Folder => Some(FsEvent::DirectoryDeleted),
                                },
                                EventKind::Any | EventKind::Other => Some(FsEvent::Modified),
                            };
                            event.map(|event| (notify_event.event.paths, event))
                        })
                        .collect();
                    debounced_events.dedup();

                    for (paths, event) in debounced_events {
                        for path in paths {
                            // we want to allow consumers to only monitor for `Modified` if they want to, so send that
                            // as well if we sent `FileDeleted`
                            if event == FsEvent::FileDeleted {
                                let _ = tx.send((path.clone(), FsEvent::Modified)).await;
                                let _ = tx.send((path, event)).await;
                            } else {
                                let _ = tx.send((path, event)).await;
                            }
                        }
                    }
                })
            },
            debouncer::NoCache,
            notify::Config::default(),
        )?;
        Ok(Self {
            debouncer: watcher,
            rx,
        })
    }

    /// Will return an error if you try to watch a file/directory which doesn't exist
    pub fn add_watcher(&mut self, dir_path: &Path) -> Result<(), NotifyStreamError> {
        // Try to use canonical paths to avoid false negatives when dealing with symlinks
        let dir_path = dir_path.canonicalize()?;
        self.debouncer.watch(&dir_path, RecursiveMode::Recursive)?;

        Ok(())
    }
}

pub fn fs_notify_stream(input: &[&Path]) -> Result<NotifyStream, NotifyStreamError> {
    let mut fs_notify = NotifyStream::try_default()?;
    for dir_path in input {
        fs_notify.add_watcher(dir_path)?;
    }
    Ok(fs_notify)
}

#[cfg(test)]
mod tests {
    use super::*;
    use maplit::hashmap;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use tedge_test_utils::fs::TempTedgeDir;
    use tedge_test_utils::fs::TempTedgeFile;

    trait DeleteWithDelayExt {
        /// Delete the file with an appropriate delay so that it falls outside the event debounce window.
        ///
        /// We need to wait before deleting the file because the debouncer will remove notifications about the file if
        /// it's created and then deleted immediately. We wait twice the debounce window so we're sure delete falls
        /// outside of it
        fn delete_with_delay(self);
    }

    impl DeleteWithDelayExt for TempTedgeFile {
        fn delete_with_delay(self) {
            std::thread::sleep(NOTIFY_STREAM_DEBOUNCE_DURATION);
            std::thread::sleep(NOTIFY_STREAM_DEBOUNCE_DURATION);
            self.delete()
        }
    }

    /// Asserts that given events will be emitted for given filenames by the `NotifyStream`.
    ///
    /// This function listens for events from `NotifyStream`, and upon receiving one, if it's present in the `inputs`
    /// hashmap, it will take it out. It will keep doing this until there are no more inputs, at which point it will
    /// return.
    async fn assert_rx_stream(
        mut inputs: HashMap<String, Vec<FsEvent>>,
        fs_notify: &mut NotifyStream,
    ) {
        while let Some((path, flag)) = fs_notify.rx.recv().await {
            let file_name = String::from(path.file_name().unwrap().to_str().unwrap());

            let Some(values) = inputs.get_mut(&file_name) else {
                continue;
            };

            match values.iter().position(|x| *x == flag) {
                Some(i) => values.remove(i),
                None => {
                    continue;
                }
            };

            if values.is_empty() {
                inputs.remove(&file_name);
            }

            if inputs.is_empty() {
                break;
            }
        }
    }

    #[cfg_attr(target_os = "macos", ignore)]
    #[tokio::test]
    async fn test_multiple_known_files_watched() {
        let ttd = Arc::new(TempTedgeDir::new());
        let ttd_clone = ttd.clone();

        let expected_events = hashmap! {
            String::from("file_a") => vec![FsEvent::FileCreated],
            String::from("file_b") => vec![FsEvent::FileCreated, FsEvent::Modified]
        };

        let mut stream = fs_notify_stream(&[ttd.path(), ttd.path()]).unwrap();

        let fs_notify_handler = tokio::task::spawn(async move {
            assert_rx_stream(expected_events, &mut stream).await;
        });

        let file_handler = tokio::task::spawn(async move {
            ttd_clone.file("file_a").with_raw_content("content");
            ttd_clone.file("file_b").with_raw_content("content");
        });

        fs_notify_handler.await.unwrap();
        file_handler.await.unwrap();
    }

    #[cfg_attr(target_os = "macos", ignore)]
    #[tokio::test]
    async fn it_works() {
        let ttd = Arc::new(TempTedgeDir::new());
        let ttd_clone = ttd.clone();
        let mut fs_notify = NotifyStream::try_default().unwrap();
        fs_notify.add_watcher(ttd.path()).unwrap();

        let assert_file_events = tokio::spawn(async move {
            let expected_events = hashmap! {
                String::from("dir_a") => vec![FsEvent::DirectoryCreated],
                String::from("file_a") => vec![FsEvent::FileCreated],
                String::from("file_b") => vec![FsEvent::FileCreated, FsEvent::Modified],
                String::from("file_c") => vec![FsEvent::FileCreated, FsEvent::FileDeleted],
            };

            let file_system_handler = async {
                ttd_clone.dir("dir_a");
                ttd_clone.file("file_a");
                ttd_clone.file("file_b");
                ttd_clone.file("file_c").delete_with_delay();
            };
            let _ = tokio::join!(
                assert_rx_stream(expected_events, &mut fs_notify),
                file_system_handler
            );
        });

        tokio::time::timeout(Duration::from_secs(1), assert_file_events)
            .await
            .unwrap()
            .unwrap();
    }

    #[cfg_attr(target_os = "macos", ignore)]
    #[tokio::test]
    async fn test_multiple_unknown_files_watched() {
        let ttd = Arc::new(TempTedgeDir::new());
        ttd.file("file_b"); // creating this file before the fs notify service
        let ttd_clone = ttd.clone();

        let expected_events = hashmap! {
            String::from("file_a") => vec![FsEvent::FileCreated],
            String::from("file_b") => vec![FsEvent::Modified],
            String::from("file_c") => vec![FsEvent::FileCreated, FsEvent::FileDeleted]
        };

        let mut stream = fs_notify_stream(&[ttd.path()]).unwrap();

        let fs_notify_handler = tokio::task::spawn(async move {
            assert_rx_stream(expected_events, &mut stream).await;
        });

        let file_handler = tokio::task::spawn(async move {
            ttd_clone.file("file_a"); // should match CREATE
            ttd_clone.file("file_b").with_raw_content("content"); // should match MODIFY
            ttd_clone.file("file_c").delete_with_delay(); // should match CREATE, DELETE
        });

        fs_notify_handler.await.unwrap();
        file_handler.await.unwrap();
    }

    #[cfg_attr(target_os = "macos", ignore)]
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
            String::from("file_a") => vec![FsEvent::FileCreated],
            String::from("file_b") => vec![FsEvent::FileCreated, FsEvent::Modified],
            String::from("file_c") => vec![FsEvent::FileCreated, FsEvent::FileDeleted],
            String::from("dir_d") => vec![FsEvent::DirectoryCreated],
        };

        let mut stream =
            fs_notify_stream(&[ttd_a.path(), ttd_b.path(), ttd_c.path(), ttd_d.path()]).unwrap();

        let fs_notify_handler = tokio::task::spawn(async move {
            assert_rx_stream(expected_events, &mut stream).await;
        });

        let file_handler = tokio::task::spawn(async move {
            ttd_a_clone.file("file_a"); // should match CREATE
            ttd_b_clone.file("file_b").with_raw_content("content"); // should match MODIFY;
            ttd_c_clone.file("file_c").delete_with_delay(); // should match CREATE, DELETE file;
            ttd_d_clone.dir("dir_d"); // should match CREATE directory;
        });

        fs_notify_handler.await.unwrap();
        file_handler.await.unwrap();
    }

    /// Make sure that `FsEvent::Modify` is emitted for various operations.
    ///
    /// In order to not duplicate various operations, consumers may want to only listen to `Modify` events, which should
    /// signify that the content of the file changed so it should be read again. However different editors and kinds of
    /// operation may result in other events, like `Rename`, `Delete`, etc. We want these operations to emit `Modify` as
    /// well, so that the consumers can only subscribe to single type of event and properly respond every time a file
    /// they're watching changes.
    #[cfg_attr(target_os = "macos", ignore)]
    #[tokio::test]
    async fn modify_emitted_for_move_copy_create_delete() {
        // Arrange
        let dir = TempTedgeDir::new();
        let nested_dir = dir.dir("nested");

        let mut stream = fs_notify_stream(&[nested_dir.path()]).unwrap();

        // Test create
        let file = nested_dir.file("file");
        assert_rx_stream(
            [(
                file.utf8_path().file_name().unwrap().to_string(),
                vec![FsEvent::Modified],
            )]
            .into(),
            &mut stream,
        )
        .await;

        // Test move to parent directory
        std::fs::rename(
            file.path(),
            dir.path().join(file.path().file_name().unwrap()),
        )
        .unwrap();
        assert_rx_stream(
            [(
                file.utf8_path().file_name().unwrap().to_string(),
                vec![FsEvent::Modified],
            )]
            .into(),
            &mut stream,
        )
        .await;

        // Test copy to directory
        std::fs::copy(
            dir.path().join(file.path().file_name().unwrap()),
            file.path(),
        )
        .unwrap();
        assert_rx_stream(
            [(
                file.utf8_path().file_name().unwrap().to_string(),
                vec![FsEvent::Modified],
            )]
            .into(),
            &mut stream,
        )
        .await;

        // Test remove
        std::fs::remove_file(file.path()).unwrap();
        assert_rx_stream(
            [(
                file.utf8_path().file_name().unwrap().to_string(),
                vec![FsEvent::Modified],
            )]
            .into(),
            &mut stream,
        )
        .await;

        // Test move from parent directory
        std::fs::rename(
            dir.path().join(file.path().file_name().unwrap()),
            file.path(),
        )
        .unwrap();
        assert_rx_stream(
            [(
                file.utf8_path().file_name().unwrap().to_string(),
                vec![FsEvent::Modified],
            )]
            .into(),
            &mut stream,
        )
        .await;
    }
}
