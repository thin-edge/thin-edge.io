use async_trait::async_trait;
use log::error;
use std::path::PathBuf;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::futures::StreamExt;
use tedge_actors::message_boxes::log_message_sent;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageSource;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_utils::notify::FsEvent;
use tedge_utils::notify::NotifyStream;
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
    fn get_watch_dirs(&self) -> &Vec<(PathBuf, DynSender<FsWatchEvent>)> {
        &self.watch_dirs
    }

    async fn send(&mut self, message: FsWatchEvent) -> Result<(), ChannelError> {
        let path = match message.clone() {
            FsWatchEvent::Modified(path) => path,
            FsWatchEvent::FileDeleted(path) => path,
            FsWatchEvent::FileCreated(path) => path,
            FsWatchEvent::DirectoryDeleted(path) => path,
            FsWatchEvent::DirectoryCreated(path) => path,
        };

        log_message_sent("Inotify", &message);

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

impl Builder<FsWatchActor> for FsWatchActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<FsWatchActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> FsWatchActor {
        let messages = FsWatchMessageBox {
            watch_dirs: self.watch_dirs,
            signal_receiver: self.signal_receiver,
        };

        FsWatchActor { messages }
    }
}
pub struct FsWatchActor {
    messages: FsWatchMessageBox,
}

#[async_trait]
impl Actor for FsWatchActor {
    fn name(&self) -> &str {
        "FsWatcher"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        let mut fs_notify = NotifyStream::try_default().map_err(Box::new)?;
        for (watch_path, _) in self.messages.get_watch_dirs().iter() {
            if let Err(err) = fs_notify.add_watcher(watch_path) {
                error!(
                    "Failed to add file watcher to the {} due to: {err}",
                    watch_path.display()
                );
            }
        }

        loop {
            tokio::select! {
                Some(RuntimeRequest::Shutdown) = self.messages.recv() => break,
                Some((path, fs_event)) = fs_notify.rx.recv() => {
                    let output = match fs_event {
                        FsEvent::Modified => FsWatchEvent::Modified(path),
                        FsEvent::FileCreated => FsWatchEvent::FileCreated(path),
                        FsEvent::FileDeleted => FsWatchEvent::FileDeleted(path),
                        FsEvent::DirectoryCreated => FsWatchEvent::DirectoryCreated(path),
                        FsEvent::DirectoryDeleted => FsWatchEvent::DirectoryDeleted(path),
                    };
                    self.messages.send(output).await?;
                }
                else => return Err(ChannelError::ReceiveError().into())
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::FsWatchActorBuilder;
    use crate::FsWatchEvent;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::Actor;
    use tedge_actors::Builder;
    use tedge_actors::DynError;
    use tedge_actors::MessageSink;
    use tedge_actors::MessageSource;
    use tedge_actors::NoMessage;
    use tedge_actors::SimpleMessageBoxBuilder;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fs_events() -> Result<(), DynError> {
        let ttd = TempTedgeDir::new();
        let mut fs_actor_builder = FsWatchActorBuilder::new();
        let client_builder: SimpleMessageBoxBuilder<FsWatchEvent, NoMessage> =
            SimpleMessageBoxBuilder::new("FS Client", 5);

        fs_actor_builder.register_peer(ttd.to_path_buf(), client_builder.get_sender());

        let mut actor = fs_actor_builder.build();
        let client_box = client_builder.build();

        tokio::spawn(async move { actor.run().await });

        // FIXME One has to wait for the actor actually launched before updating the file system.
        //       - Do we need some message sent by the actors to say that are ready?
        //       - Do we need a more sophisticated actor that list the existing files on start?
        tokio::time::sleep(Duration::from_millis(100)).await;
        ttd.file("file_a");
        ttd.dir("dir_b").file("file_b");

        client_box
            .with_timeout(TEST_TIMEOUT)
            .assert_received_unordered([
                FsWatchEvent::Modified(ttd.to_path_buf().join("file_a")),
                FsWatchEvent::DirectoryCreated(ttd.to_path_buf().join("dir_b")),
                FsWatchEvent::Modified(ttd.to_path_buf().join("dir_b").join("file_b")),
            ])
            .await;

        Ok(())
    }
}
