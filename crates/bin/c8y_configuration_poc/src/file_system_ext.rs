use async_trait::async_trait;
use std::path::PathBuf;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;
use tedge_actors::MessageSource;
use tedge_actors::NoMessage;
use tedge_utils::notify::FsEvent;
use tedge_utils::notify::NotifyStream;
use tokio::sync::mpsc::Receiver;
use try_traits::default::TryDefault;
use try_traits::Infallible;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FsWatchEvent {
    Modified(PathBuf),
    FileDeleted(PathBuf),
    FileCreated(PathBuf),
    DirectoryDeleted(PathBuf),
    DirectoryCreated(PathBuf),
}

pub struct FsWatchMessageBox {
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
    type Input = NoMessage;
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

impl Builder<(FsWatchActor, FsWatchMessageBox)> for FsWatchActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<(FsWatchActor, FsWatchMessageBox), Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> (FsWatchActor, FsWatchMessageBox) {
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

        (fs_event_actor, mailbox)
    }
}
pub struct FsWatchActor {
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
