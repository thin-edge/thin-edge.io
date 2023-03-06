use async_trait::async_trait;
use std::path::PathBuf;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::futures::StreamExt;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;
use tedge_actors::MessageSource;
use tedge_actors::NoMessage;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_utils::notify::FsEvent;
use tedge_utils::notify::NotifyStream;
use tokio::sync::mpsc::Receiver;
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
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
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

    async fn recv(&mut self) -> Option<RuntimeRequest> {
        self.signal_receiver.next().await
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
    signal_sender: mpsc::Sender<RuntimeRequest>,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
}

impl FsWatchActorBuilder {
    pub fn new() -> Self {
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        Self {
            watch_dirs: Vec::new(),
            signal_sender,
            signal_receiver,
        }
    }
}

impl Default for FsWatchActorBuilder {
    fn default() -> Self {
        FsWatchActorBuilder::new()
    }
}

impl MessageSource<FsWatchEvent, PathBuf> for FsWatchActorBuilder {
    fn register_peer(&mut self, config: PathBuf, sender: DynSender<FsWatchEvent>) {
        self.watch_dirs.push((config, sender));
    }
}

impl RuntimeRequestSink for FsWatchActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.signal_sender.clone())
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
            signal_receiver: self.signal_receiver,
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

    async fn run(mut self, mut messages: Self::MessageBox) -> Result<(), RuntimeError> {
        loop {
            tokio::select! {
                Some(RuntimeRequest::Shutdown) = messages.recv() => return Err(ChannelError::ReceiveError().into()),
                Some((path, fs_event)) = self.fs_notify_receiver.recv() => {
                    let output = match fs_event {
                        FsEvent::Modified => FsWatchEvent::Modified(path),
                        FsEvent::FileCreated => FsWatchEvent::FileCreated(path),
                        FsEvent::FileDeleted => FsWatchEvent::FileDeleted(path),
                        FsEvent::DirectoryCreated => FsWatchEvent::DirectoryCreated(path),
                        FsEvent::DirectoryDeleted => FsWatchEvent::DirectoryDeleted(path),
                    };
                    messages.send(output).await?;
                }
                else => return Err(ChannelError::ReceiveError().into())
            }
        }
    }
}
