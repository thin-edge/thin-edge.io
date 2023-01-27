use async_trait::async_trait;
use std::path::PathBuf;
use tedge_actors::Actor;
use tedge_actors::ActorBuilder;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;
use tedge_actors::MessageSource;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_utils::notify::FsEvent;
use tedge_utils::notify::NotifyStream;
use tokio::sync::mpsc::Receiver;
use try_traits::default::TryDefault;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FsWatchEvent {
    Modified(PathBuf),
    FileDeleted(PathBuf),
    FileCreated(PathBuf),
    DirectoryDeleted(PathBuf),
    DirectoryCreated(PathBuf),
}

#[derive(Debug)]
enum NullInput {}

struct FsWatchMessageBox {
    watch_dirs: Vec<(PathBuf, DynSender<FsWatchEvent>)>,
}

impl FsWatchMessageBox {
    async fn send(&mut self, message: FsWatchEvent) -> Result<(), ChannelError> {
        let path = match message.clone() {
            FsWatchEvent::Modified(path) => path,
            FsWatchEvent::FileDeleted(path) => path,
            FsWatchEvent::FileCreated(path) => path,
            FsWatchEvent::DirectoryDeleted(path) => path,
            FsWatchEvent::DirectoryCreated(path) => path,
        };

        self.log_output(&message);
        for (watch_path, sender) in self.watch_dirs.iter_mut() {
            if path.starts_with(watch_path) {
                sender.send(message.clone()).await?;
            }
        }

        Ok(())
    }
}

impl MessageBox for FsWatchMessageBox {
    type Input = NullInput;
    type Output = FsWatchEvent;

    fn turn_logging_on(&mut self, _on: bool) {}

    fn name(&self) -> &str {
        "Inotify"
    }

    fn logging_is_on(&self) -> bool {
        true
    }
}

pub struct FsWatchActorBuilder {
    watch_dirs: Vec<(PathBuf, DynSender<FsWatchEvent>)>,
}

impl FsWatchActorBuilder {
    pub fn new() -> Self {
        Self {
            watch_dirs: Vec::new(),
        }
    }
}

impl MessageSource<FsWatchEvent, PathBuf> for FsWatchActorBuilder {
    fn register_peer(&mut self, config: PathBuf, sender: DynSender<FsWatchEvent>) {
        self.watch_dirs.push((config, sender));
    }
}

#[async_trait]
impl ActorBuilder for FsWatchActorBuilder {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let mut fs_notify = NotifyStream::try_default().unwrap();
        for (watch_path, _) in self.watch_dirs.iter() {
            fs_notify
                .add_watcher(
                    watch_path,
                    None,
                    &[
                        FsEvent::Modified,
                        FsEvent::FileDeleted,
                        FsEvent::FileCreated,
                    ],
                )
                .unwrap();
        }

        let fs_event_actor = FsWatchActor {
            fs_notify_receiver: fs_notify.rx,
        };

        let mailbox = FsWatchMessageBox {
            watch_dirs: self.watch_dirs,
        };

        runtime.run(fs_event_actor, mailbox).await?;

        Ok(())
    }
}
struct FsWatchActor {
    fs_notify_receiver: Receiver<(PathBuf, FsEvent)>,
}

#[async_trait]
impl Actor for FsWatchActor {
    type MessageBox = FsWatchMessageBox;

    fn name(&self) -> &str {
        "FsWatcher"
    }

    async fn run(mut self, mut mailbox: Self::MessageBox) -> Result<(), ChannelError> {
        loop {
            if let Some((path, fs_event)) = self.fs_notify_receiver.recv().await {
                let output = match fs_event {
                    FsEvent::Modified => FsWatchEvent::Modified(path),
                    FsEvent::FileCreated => FsWatchEvent::FileCreated(path),
                    FsEvent::FileDeleted => FsWatchEvent::FileDeleted(path),
                    FsEvent::DirectoryCreated => FsWatchEvent::DirectoryCreated(path),
                    FsEvent::DirectoryDeleted => FsWatchEvent::DirectoryDeleted(path),
                };
                mailbox.send(output).await?;
            } else {
                return Err(ChannelError::ReceiveError());
            }
        }
    }
}
