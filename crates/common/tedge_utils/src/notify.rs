use notify::event::AccessKind;
use notify::event::AccessMode;
use notify::event::CreateKind;
use notify::event::ModifyKind;
use notify::event::RemoveKind;
use notify::Config;
use notify::EventKind;
use notify::INotifyWatcher;
use notify::RecommendedWatcher;
use notify::RecursiveMode;
use notify::Watcher;
use std::hash::Hash;
use std::path::Path;
use std::path::PathBuf;
use strum_macros::Display;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::Receiver;

#[derive(Debug, Display, PartialEq, Eq, Clone, Hash, PartialOrd, Ord, Copy)]
pub enum FsEvent {
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
    watcher: INotifyWatcher,
    pub rx: Receiver<(PathBuf, FsEvent)>,
}

impl NotifyStream {
    pub fn try_default() -> Result<Self, NotifyStreamError> {
        let (tx, rx) = channel(1024);

        // Automatically select the best implementation for your platform.
        // You can also access each implementation directly e.g. INotifyWatcher.
        let watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                futures::executor::block_on(async {
                    if let Ok(notify_event) = res {
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
                            EventKind::Modify(modify_kind) => match modify_kind {
                                ModifyKind::Data(_)
                                | ModifyKind::Metadata(_)
                                | ModifyKind::Name(_)
                                | ModifyKind::Any
                                | ModifyKind::Other => Some(FsEvent::Modified),
                            },
                            EventKind::Remove(remove_kind) => match remove_kind {
                                RemoveKind::File | RemoveKind::Any | RemoveKind::Other => {
                                    Some(FsEvent::FileDeleted)
                                }
                                RemoveKind::Folder => Some(FsEvent::DirectoryDeleted),
                            },
                            EventKind::Any | EventKind::Other => Some(FsEvent::Modified),
                        };

                        if let Some(event) = event {
                            for path in notify_event.paths {
                                let _ = tx.send((path, event)).await;
                            }
                        }
                    }
                })
            },
            Config::default(),
        )?;
        Ok(Self { watcher, rx })
    }

    /// Will return an error if you try to watch a file/directory which doesn't exist
    pub fn add_watcher(&mut self, dir_path: &Path) -> Result<(), NotifyStreamError> {
        self.watcher.watch(dir_path, RecursiveMode::Recursive)?;

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

    async fn assert_rx_stream(
        mut inputs: HashMap<String, Vec<FsEvent>>,
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
            String::from("file_a") => vec![FsEvent::FileCreated],
            String::from("file_b") => vec![FsEvent::FileCreated, FsEvent::Modified]
        };

        let stream = fs_notify_stream(&[ttd.path(), ttd.path()]).unwrap();

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
                ttd_clone.file("file_c").delete();
            };
            let _ = tokio::join!(
                assert_rx_stream(expected_events, fs_notify),
                file_system_handler
            );
        });

        tokio::time::timeout(Duration::from_secs(1), assert_file_events)
            .await
            .unwrap()
            .unwrap();
    }

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

        let stream = fs_notify_stream(&[ttd.path()]).unwrap();

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
            String::from("file_a") => vec![FsEvent::FileCreated],
            String::from("file_b") => vec![FsEvent::FileCreated, FsEvent::Modified],
            String::from("file_c") => vec![FsEvent::FileCreated, FsEvent::FileDeleted],
            String::from("dir_d") => vec![FsEvent::DirectoryCreated],
        };

        let stream =
            fs_notify_stream(&[ttd_a.path(), ttd_b.path(), ttd_c.path(), ttd_d.path()]).unwrap();

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
