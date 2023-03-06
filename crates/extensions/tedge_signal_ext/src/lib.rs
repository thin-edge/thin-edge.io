use async_trait::async_trait;
use signal_hook::consts::signal::*;
use signal_hook_tokio::Signals;
use std::convert::Infallible;
use tedge_actors::futures::StreamExt;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::NoMessage;
use tedge_actors::RuntimeAction;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;

pub type SignalMessageBox = SimpleMessageBox<NoMessage, RuntimeAction>;

pub struct SignalActorBuilder {
    box_builder: SimpleMessageBoxBuilder<NoMessage, RuntimeAction>,
}

impl Builder<(SignalActor, SignalMessageBox)> for SignalActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<(SignalActor, SignalMessageBox), Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> (SignalActor, SignalMessageBox) {
        (SignalActor, self.box_builder.build())
    }
}

impl RuntimeRequestSink for SignalActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl MessageSource<RuntimeAction, NoConfig> for SignalActorBuilder {
    fn register_peer(&mut self, config: NoConfig, sender: DynSender<RuntimeAction>) {
        self.box_builder.register_peer(config, sender)
    }
}

pub struct SignalActor;

impl SignalActor {
    pub fn builder() -> SignalActorBuilder {
        let box_builder = SimpleMessageBoxBuilder::new("Signal-Handler", 1);
        SignalActorBuilder { box_builder }
    }
}

#[async_trait]
impl Actor for SignalActor {
    type MessageBox = SignalMessageBox;

    fn name(&self) -> &str {
        "Signal-Handler"
    }

    async fn run(self, mut messages: Self::MessageBox) -> Result<(), RuntimeError> {
        let mut signals = Signals::new([SIGTERM, SIGINT, SIGQUIT]).unwrap(); // FIXME
        loop {
            tokio::select! {
                None = messages.recv() => return Ok(()),
                Some(signal) = signals.next() => {
                    match signal {
                        SIGTERM | SIGINT | SIGQUIT => messages.send(RuntimeAction::Shutdown).await?,
                        _ => unreachable!(),
                    }
                }
                else => return Ok(())
            }
        }
    }
}
