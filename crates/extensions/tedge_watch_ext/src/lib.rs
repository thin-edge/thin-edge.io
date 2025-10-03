use crate::actor::Watcher;
use camino::Utf8PathBuf;
use std::convert::Infallible;
use tedge_actors::Builder;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBoxBuilder;

mod actor;
#[cfg(test)]
mod tests;

#[derive(Debug)]
pub enum WatchRequest {
    WatchFile { topic: String, file: Utf8PathBuf },
    WatchCommand { topic: String, command: String },
    UnWatch { topic: String },
}

#[derive(Debug)]
pub enum WatchEvent {
    NewLine { topic: String, line: String },
    EndOfStream { topic: String },
    Error { topic: String, error: WatchError },
}

#[derive(thiserror::Error, Debug)]
pub enum WatchError {
    #[error("Invalid command `{command}`: {error}")]
    InvalidCommand { command: String, error: String },

    #[error("Failed to execute `{command}`: {error}")]
    ExecutionFailed { command: String, error: String },

    #[error("Failed to kill `{command}`: {error}")]
    TerminationFailed { command: String, error: String },
}

pub struct WatchActorBuilder {
    message_box: SimpleMessageBoxBuilder<WatchRequest, WatchEvent>,
}

impl WatchActorBuilder {
    pub fn new() -> Self {
        let message_box = SimpleMessageBoxBuilder::new("watcher", 16);
        WatchActorBuilder { message_box }
    }

    pub fn connect(
        &mut self,
        client: &mut (impl MessageSource<WatchRequest, NoConfig> + MessageSink<WatchEvent>),
    ) {
        self.connect_source(NoConfig, client);
        self.connect_sink(NoConfig, client);
    }
}

impl MessageSink<WatchRequest> for WatchActorBuilder {
    fn get_sender(&self) -> DynSender<WatchRequest> {
        self.message_box.get_sender()
    }
}

impl MessageSource<WatchEvent, NoConfig> for WatchActorBuilder {
    fn connect_sink(&mut self, config: NoConfig, peer: &impl MessageSink<WatchEvent>) {
        self.message_box.connect_sink(config, peer);
    }
}

impl RuntimeRequestSink for WatchActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.message_box.get_signal_sender()
    }
}

impl Builder<Watcher> for WatchActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<Watcher, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> Watcher {
        let request_sender = self.message_box.get_sender();
        let (event_sender, request_receiver) = self.message_box.build().into_split();
        Watcher::new(
            event_sender.sender_clone(),
            request_sender,
            request_receiver,
        )
    }
}

impl Default for WatchActorBuilder {
    fn default() -> Self {
        WatchActorBuilder::new()
    }
}
