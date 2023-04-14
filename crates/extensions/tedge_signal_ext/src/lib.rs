use async_trait::async_trait;
use signal_hook::consts::signal::*;
use signal_hook_tokio::Signals;
use std::convert::Infallible;
use tedge_actors::futures::StreamExt;
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
    fn register_peer(&mut self, config: NoConfig, sender: DynSender<RuntimeAction>) {
        self.box_builder.register_peer(config, sender)
    }
}

pub struct SignalActor {
    messages: SignalMessageBox,
}

impl SignalActor {
    pub fn builder(runtime: &impl MessageSink<RuntimeAction, NoConfig>) -> SignalActorBuilder {
        let mut box_builder = SimpleMessageBoxBuilder::new("Signal-Handler", 1);
        box_builder.add_sink(runtime);
        SignalActorBuilder { box_builder }
    }
}

#[async_trait]
impl Actor for SignalActor {
    fn name(&self) -> &str {
        "Signal-Handler"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        let mut signals = Signals::new([SIGTERM, SIGINT, SIGQUIT]).unwrap(); // FIXME
        loop {
            tokio::select! {
                None = self.messages.recv() => return Ok(()),
                Some(signal) = signals.next() => {
                    match signal {
                        SIGTERM | SIGINT | SIGQUIT => self.messages.send(RuntimeAction::Shutdown).await?,
                        _ => unreachable!(),
                    }
                }
                else => return Ok(())
            }
        }
    }
}
