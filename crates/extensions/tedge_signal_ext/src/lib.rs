use async_trait::async_trait;
use signal_hook::consts::signal::*;
use signal_hook_tokio::Signals;
use tedge_actors::Actor;
use tedge_actors::ActorBuilder;
use tedge_actors::ChannelError;
use tedge_actors::MessageBox;
use tedge_actors::RuntimeAction;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_actors::StreamExt;

pub struct SignalActorBuilder;

#[async_trait]
impl ActorBuilder for SignalActorBuilder {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let message_box = SignalMessageBox::new(runtime.clone());
        runtime.run(SignalActor, message_box).await
    }
}

pub struct SignalActor;

impl SignalActor {
    pub fn builder() -> SignalActorBuilder {
        SignalActorBuilder
    }
}

#[async_trait]
impl Actor for SignalActor {
    type MessageBox = SignalMessageBox;

    fn name(&self) -> &str {
        "Signal-Handler"
    }

    async fn run(self, mut messages: Self::MessageBox) -> Result<(), ChannelError> {
        while let Some(signal) = messages.recv().await {
            match signal {
                SIGTERM | SIGINT | SIGQUIT => messages.send(RuntimeAction::Shutdown).await?,
                _ => unreachable!(),
            }
        }
        Ok(())
    }
}

pub struct SignalMessageBox {
    runtime: RuntimeHandle,
    signals: Signals,
}

impl SignalMessageBox {
    fn new(runtime: RuntimeHandle) -> Self {
        let signals = Signals::new(&[SIGTERM, SIGINT, SIGQUIT]).unwrap(); // FIXME
        SignalMessageBox { runtime, signals }
    }
}

#[async_trait]
impl MessageBox for SignalMessageBox {
    type Input = i32;
    type Output = RuntimeAction;

    async fn recv(&mut self) -> Option<Self::Input> {
        self.signals.next().await
    }

    async fn send(&mut self, message: Self::Output) -> Result<(), ChannelError> {
        self.log_output(&message);
        self.runtime.send(message).await
    }

    fn turn_logging_on(&mut self, _on: bool) {
        todo!()
    }

    fn name(&self) -> &str {
        "Signal-Handler"
    }

    fn logging_is_on(&self) -> bool {
        true
    }
}
