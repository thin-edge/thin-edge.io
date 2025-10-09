use crate::WatchError;
use crate::WatchEvent;
use crate::WatchRequest;
use camino::Utf8PathBuf;
use std::collections::HashMap;
use std::os::unix::prelude::ExitStatusExt;
use std::process::Stdio;
use tedge_actors::Actor;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::LoggingReceiver;
use tedge_actors::MessageReceiver;
use tedge_actors::NullSender;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::ChildStderr;
use tokio::process::ChildStdout;
use tokio::process::Command;

type ClientId = u32;
type Topic = String;
type CommandLine = String;

pub struct Watcher {
    /// The collection of commands watched by each client
    processes: HashMap<(ClientId, Topic), (CommandLine, Child)>,
    /// The channels to send events to clients identified by their slot
    event_senders: Vec<DynSender<WatchEvent>>,
    /// Channel used to send requests on behalf of a client
    request_sender: DynSender<(ClientId, WatchRequest)>,
    /// Request received from a client
    request_receiver: LoggingReceiver<(ClientId, WatchRequest)>,
}

#[async_trait::async_trait]
impl Actor for Watcher {
    fn name(&self) -> &str {
        "Watcher"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        while let Some((client, request)) = self.request_receiver.recv().await {
            let topic = match &request {
                WatchRequest::WatchFile { topic, .. }
                | WatchRequest::WatchCommand { topic, .. }
                | WatchRequest::UnWatch { topic } => topic.clone(),
            };
            let result = match request {
                WatchRequest::WatchFile { topic, file } => {
                    self.watch_file(client, topic, file).await
                }
                WatchRequest::WatchCommand { topic, command } => {
                    self.watch_command(client, topic, command).await
                }
                WatchRequest::UnWatch { topic } => self.unwatch(client, topic).await,
            };
            if let Err(error) = result {
                self.client_sender(client)
                    .send(WatchEvent::Error { topic, error })
                    .await?;
            }
        }
        Ok(())
    }
}

impl Watcher {
    pub fn new(
        event_senders: Vec<DynSender<WatchEvent>>,
        request_sender: DynSender<(ClientId, WatchRequest)>,
        request_receiver: LoggingReceiver<(ClientId, WatchRequest)>,
    ) -> Self {
        Watcher {
            processes: HashMap::new(),
            event_senders,
            request_sender,
            request_receiver,
        }
    }

    pub async fn watch_file(
        &mut self,
        client: u32,
        topic: Topic,
        file: Utf8PathBuf,
    ) -> Result<(), WatchError> {
        let command = format!("tail -F {file}");
        self.watch_command(client, topic, command).await
    }

    pub async fn watch_command(
        &mut self,
        client: u32,
        topic: Topic,
        command: String,
    ) -> Result<(), WatchError> {
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
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| WatchError::InvalidCommand {
                command: command.clone(),
                error: err.to_string(),
            })?;

        if let Some(stdout) = child.stdout.take() {
            self.spawn_stdout_reader(client, topic.clone(), stdout);
        }
        if let Some(stderr) = child.stderr.take() {
            self.spawn_stderr_reader(client, topic.clone(), stderr);
        }

        self.processes.insert((client, topic), (command, child));
        Ok(())
    }

    fn client_sender(&self, client: u32) -> DynSender<WatchEvent> {
        self.event_senders
            .get(client as usize)
            .map(|s| s.sender_clone())
            .unwrap_or(NullSender.into())
    }

    fn spawn_stdout_reader(&self, client: ClientId, topic: Topic, stdout: ChildStdout) {
        let mut event_sender = self.client_sender(client);
        let mut request_sender: DynSender<(ClientId, WatchRequest)> =
            self.request_sender.sender_clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = event_sender
                    .send(WatchEvent::StdoutLine {
                        topic: topic.clone(),
                        line,
                    })
                    .await;
            }
            let _ = request_sender
                .send((
                    client,
                    WatchRequest::UnWatch {
                        topic: topic.clone(),
                    },
                ))
                .await;
            let _ = event_sender.send(WatchEvent::EndOfStream { topic }).await;
        });
    }

    fn spawn_stderr_reader(&self, client: ClientId, topic: Topic, stderr: ChildStderr) {
        let mut event_sender = self.client_sender(client);
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = event_sender
                    .send(WatchEvent::StderrLine {
                        topic: topic.clone(),
                        line,
                    })
                    .await;
            }
        });
    }

    pub async fn unwatch(&mut self, client: u32, topic: Topic) -> Result<(), WatchError> {
        if let Some((command, mut child)) = self.processes.remove(&(client, topic)) {
            if let Ok(Some(exit_code)) = child.try_wait() {
                if exit_code.success() {
                    return Ok(());
                }
                match exit_code.code() {
                    Some(exit_code) => {
                        return Err(WatchError::CommandFailed { command, exit_code })
                    }
                    None => {
                        return Err(WatchError::CommandKilled {
                            command,
                            signal: exit_code.signal().unwrap_or_default(),
                        })
                    }
                }
            }
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
