use crate::WatchError;
use crate::WatchEvent;
use crate::WatchRequest;
use camino::Utf8PathBuf;
use std::collections::HashMap;
use std::process::Stdio;
use tedge_actors::Actor;
use tedge_actors::DynSender;
use tedge_actors::LoggingReceiver;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::ChildStdout;
use tokio::process::Command;

type Topic = String;
type CommandLine = String;

pub struct Watcher {
    processes: HashMap<Topic, (CommandLine, Child)>,
    event_sender: DynSender<WatchEvent>,
    request_sender: DynSender<WatchRequest>,
    request_receiver: LoggingReceiver<WatchRequest>,
}

#[async_trait::async_trait]
impl Actor for Watcher {
    fn name(&self) -> &str {
        "Watcher"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        while let Some(request) = self.request_receiver.recv().await {
            let topic = match &request {
                WatchRequest::WatchFile { topic, .. }
                | WatchRequest::WatchCommand { topic, .. }
                | WatchRequest::UnWatch { topic } => topic.clone(),
            };
            let result = match request {
                WatchRequest::WatchFile { topic, file } => self.watch_file(topic, file).await,
                WatchRequest::WatchCommand { topic, command } => {
                    self.watch_command(topic, command).await
                }
                WatchRequest::UnWatch { topic } => self.unwatch(&topic).await,
            };
            if let Err(error) = result {
                self.event_sender
                    .send(WatchEvent::Error { topic, error })
                    .await?;
            }
        }
        Ok(())
    }
}

impl Watcher {
    pub fn new(
        event_sender: DynSender<WatchEvent>,
        request_sender: DynSender<WatchRequest>,
        request_receiver: LoggingReceiver<WatchRequest>,
    ) -> Self {
        Watcher {
            processes: HashMap::new(),
            event_sender,
            request_sender,
            request_receiver,
        }
    }

    pub async fn watch_file(&mut self, topic: Topic, file: Utf8PathBuf) -> Result<(), WatchError> {
        let command = format!("tail -F {file}");
        self.watch_command(topic, command).await
    }

    pub async fn watch_command(&mut self, topic: Topic, command: String) -> Result<(), WatchError> {
        let args = shell_words::split(&command).map_err(|err| WatchError::InvalidCommand {
            command: command.clone(),
            error: err.to_string(),
        })?;
        if args.is_empty() {
            return Err(WatchError::InvalidCommand {
                command,
                error: "Empty command".to_string(),
            });
        }
        let mut child = Command::new(&args[0])
            .args(&args[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| WatchError::InvalidCommand {
                command: command.clone(),
                error: err.to_string(),
            })?;

        if let Some(stdout) = child.stdout.take() {
            self.spawn_reader(topic.clone(), stdout);
        }

        self.processes.insert(topic, (command, child));
        Ok(())
    }

    fn spawn_reader(&self, topic: Topic, stdout: ChildStdout) {
        let mut event_sender = self.event_sender.sender_clone();
        let mut request_sender = self.request_sender.sender_clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = event_sender
                    .send(WatchEvent::NewLine {
                        topic: topic.clone(),
                        line,
                    })
                    .await;
            }
            let _ = event_sender
                .send(WatchEvent::EndOfStream {
                    topic: topic.clone(),
                })
                .await;
            let _ = request_sender.send(WatchRequest::UnWatch { topic }).await;
        });
    }

    pub async fn unwatch(&mut self, topic: &Topic) -> Result<(), WatchError> {
        if let Some((command, mut child)) = self.processes.remove(topic) {
            child
                .kill()
                .await
                .map_err(|err| WatchError::TerminationFailed {
                    command,
                    error: err.to_string(),
                })?;
        }
        Ok(())
    }
}
