use crate::actor::Watcher;
use camino::Utf8PathBuf;
use std::convert::Infallible;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::NoMessage;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBoxBuilder;

mod actor;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WatchRequest {
    WatchFile { topic: String, file: Utf8PathBuf },
    WatchCommand { topic: String, command: String },
    UnWatch { topic: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WatchEvent {
    StdoutLine { topic: String, line: String },
    StderrLine { topic: String, line: String },
    EndOfStream { topic: String },
    Error { topic: String, error: WatchError },
}

#[derive(thiserror::Error, Clone, Debug, Eq, PartialEq)]
pub enum WatchError {
    #[error("Invalid command `{command}`: {error}")]
    InvalidCommand { command: String, error: String },

    #[error("Failed to execute `{command}`: {error}")]
    ExecutionFailed { command: String, error: String },

    #[error("Exited with exit-code {exit_code}: `{command}`")]
    CommandFailed { command: String, exit_code: i32 },

    #[error("Command killed by signal {signal}: `{command}`")]
    CommandKilled { command: String, signal: i32 },

    #[error("Failed to kill `{command}`: {error}")]
    TerminationFailed { command: String, error: String },
}

pub use actor::command_output;

pub struct WatchActorBuilder {
    request_box: SimpleMessageBoxBuilder<(u32, WatchRequest), NoMessage>,
    event_senders: Vec<DynSender<WatchEvent>>,
}

impl WatchActorBuilder {
    pub fn new() -> Self {
        let request_box = SimpleMessageBoxBuilder::new("watcher", 16);
        let event_senders = vec![];
        WatchActorBuilder {
            request_box,
            event_senders,
        }
    }

    pub fn connect(
        &mut self,
        client: &mut (impl MessageSource<WatchRequest, NoConfig> + MessageSink<WatchEvent>),
    ) {
        let client_id = self.event_senders.len() as u32;
        self.event_senders.push(client.get_sender());
        client.connect_mapped_sink(NoConfig, self, move |req| Some((client_id, req)));
    }
}

impl MessageSink<(u32, WatchRequest)> for WatchActorBuilder {
    fn get_sender(&self) -> DynSender<(u32, WatchRequest)> {
        self.request_box.get_sender()
    }
}

impl RuntimeRequestSink for WatchActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.request_box.get_signal_sender()
    }
}

impl Builder<Watcher> for WatchActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<Watcher, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> Watcher {
        let request_sender = self.request_box.get_sender();
        let (_, request_receiver) = self.request_box.build().into_split();
        Watcher::new(self.event_senders, request_sender, request_receiver)
    }
}

impl Default for WatchActorBuilder {
    fn default() -> Self {
        WatchActorBuilder::new()
    }
}
