use async_trait::async_trait;
use std::convert::Infallible;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::NoMessage;
use tedge_actors::RuntimeAction;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tokio::signal::unix;

pub type SignalMessageBox = SimpleMessageBox<NoMessage, RuntimeAction>;

pub struct SignalActorBuilder {
    box_builder: SimpleMessageBoxBuilder<NoMessage, RuntimeAction>,
}

impl Builder<SignalActor> for SignalActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<SignalActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> SignalActor {
        SignalActor {
            messages: self.box_builder.build(),
        }
    }
}

impl RuntimeRequestSink for SignalActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl MessageSource<RuntimeAction, NoConfig> for SignalActorBuilder {
    fn connect_sink(&mut self, config: NoConfig, peer: &impl MessageSink<RuntimeAction>) {
        self.box_builder.connect_sink(config, peer)
    }
}

pub struct SignalActor {
    messages: SignalMessageBox,
}

impl SignalActor {
    pub fn builder(runtime: &impl MessageSink<RuntimeAction>) -> SignalActorBuilder {
        let mut box_builder = SimpleMessageBoxBuilder::new("Signal-Handler", 1);
        box_builder.connect_sink(NoConfig, runtime);
        SignalActorBuilder { box_builder }
    }
}

#[async_trait]
impl Actor for SignalActor {
    fn name(&self) -> &str {
        "Signal-Handler"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let mut sig_int = unix::signal(unix::SignalKind::interrupt())
            .map_err(|e| RuntimeError::ActorError(e.into()))?;
        let mut sig_term = unix::signal(unix::SignalKind::terminate())
            .map_err(|e| RuntimeError::ActorError(e.into()))?;
        let mut sig_quit = unix::signal(unix::SignalKind::quit())
            .map_err(|e| RuntimeError::ActorError(e.into()))?;
        loop {
            tokio::select! {
                _ = self.messages.recv() => return Ok(()),
                _ = sig_int.recv() => {
                    self.messages.send(RuntimeAction::Shutdown).await?
                }
                _ = sig_term.recv() => {
                    self.messages.send(RuntimeAction::Shutdown).await?
                }
                _ = sig_quit.recv() => {
                    self.messages.send(RuntimeAction::Shutdown).await?
                },
            }
        }
    }
}
