use crate::actor::TimerActor;
use crate::AnyPayload;
use crate::SetTimeout;
use crate::Timeout;
use async_trait::async_trait;
use std::convert::Infallible;
use std::marker::PhantomData;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::Message;
use tedge_actors::MessageBoxPlug;
use tedge_actors::MessageBoxSocket;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sender;
use tedge_actors::ServiceMessageBoxBuilder;

pub struct TimerActorBuilder {
    box_builder: ServiceMessageBoxBuilder<SetTimeout<AnyPayload>, Timeout<AnyPayload>>,
}

impl Default for TimerActorBuilder {
    fn default() -> Self {
        TimerActorBuilder {
            box_builder: ServiceMessageBoxBuilder::new("Timer", 16),
        }
    }
}

impl Builder<(TimerActor, <TimerActor as Actor>::MessageBox)> for TimerActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<(TimerActor, <TimerActor as Actor>::MessageBox), Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> (TimerActor, <TimerActor as Actor>::MessageBox) {
        let actor = TimerActor::default();
        let actor_box = self.box_builder.build();
        (actor, actor_box)
    }
}

impl RuntimeRequestSink for TimerActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl<T: Message> MessageBoxSocket<SetTimeout<T>, Timeout<T>, NoConfig> for TimerActorBuilder {
    fn connect_with(
        &mut self,
        peer: &mut impl MessageBoxPlug<SetTimeout<T>, Timeout<T>>,
        config: NoConfig,
    ) {
        let mut adapter = AnyTimerAdapter::new(peer);
        self.box_builder.connect_with(&mut adapter, config);
    }
}

/// A message adapter used by actors to send timer requests with a generic payload `SetTimeout<T>`
/// and to receive accordingly timer responses with a generic payload `Timeout<T>`,
/// while the timer actor only handles opaque payloads of type `Box<dyn Any>`.
struct AnyTimerAdapter<'a, T: Message, Plug: MessageBoxPlug<SetTimeout<T>, Timeout<T>>> {
    inner: &'a mut Plug,
    _phantom: PhantomData<T>,
}

impl<'a, T: Message, Plug: MessageBoxPlug<SetTimeout<T>, Timeout<T>>> AnyTimerAdapter<'a, T, Plug> {
    fn new(inner: &'a mut Plug) -> Self {
        Self {
            inner,
            _phantom: PhantomData,
        }
    }
}

impl<'a, T, Plug> MessageBoxPlug<SetTimeout<AnyPayload>, Timeout<AnyPayload>>
    for AnyTimerAdapter<'a, T, Plug>
where
    T: Message,
    Plug: MessageBoxPlug<SetTimeout<T>, Timeout<T>>,
{
    fn set_request_sender(&mut self, request_sender: DynSender<SetTimeout<AnyPayload>>) {
        self.inner.set_request_sender(Box::new(SetTimeoutSender {
            inner: request_sender,
        }))
    }

    fn get_response_sender(&self) -> DynSender<Timeout<AnyPayload>> {
        Box::new(TimeoutSender {
            inner: self.inner.get_response_sender(),
        })
    }
}

/// A Sender that translates timeout responses on the wire
///
/// This sender receives `Timeout<AnyPayload>` from the `TimerActor`,
/// and translates then forwards these messages to an actor expecting `Timeout<T>`
struct TimeoutSender<T: Message> {
    inner: DynSender<Timeout<T>>,
}

#[async_trait]
impl<T: Message> Sender<Timeout<AnyPayload>> for TimeoutSender<T> {
    async fn send(&mut self, message: Timeout<AnyPayload>) -> Result<(), ChannelError> {
        if let Ok(event) = message.event.downcast() {
            self.inner.send(Timeout { event: *event }).await?;
        }
        Ok(())
    }

    fn sender_clone(&self) -> DynSender<Timeout<AnyPayload>> {
        Box::new(TimeoutSender {
            inner: self.inner.sender_clone(),
        })
    }

    fn close_sender(&mut self) {
        self.inner.as_mut().close_sender()
    }
}

/// A Sender that translates timeout requests on the wire
///
/// This sender receives `SetTimeout<T>` requests from some actor,
/// and translates then forwards these messages to the timer actor expecting`Timeout<AnyPayload>`
struct SetTimeoutSender {
    inner: DynSender<SetTimeout<AnyPayload>>,
}

#[async_trait]
impl<T: Message> Sender<SetTimeout<T>> for SetTimeoutSender {
    async fn send(&mut self, request: SetTimeout<T>) -> Result<(), ChannelError> {
        let duration = request.duration;
        let event: AnyPayload = Box::new(request.event);
        self.inner.send(SetTimeout { duration, event }).await
    }

    fn sender_clone(&self) -> DynSender<SetTimeout<T>> {
        Box::new(SetTimeoutSender {
            inner: self.inner.sender_clone(),
        })
    }

    fn close_sender(&mut self) {
        self.inner.as_mut().close_sender()
    }
}
