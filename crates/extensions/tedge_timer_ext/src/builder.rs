use crate::actor::TimerActor;
use crate::AnyPayload;
use crate::SetTimeout;
use crate::Timeout;
use async_trait::async_trait;
use std::convert::Infallible;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::Message;
use tedge_actors::MessageSink;
use tedge_actors::RequestEnvelope;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sender;
use tedge_actors::ServerMessageBoxBuilder;

pub struct TimerActorBuilder {
    box_builder: ServerMessageBoxBuilder<SetTimeout<AnyPayload>, Timeout<AnyPayload>>,
}

impl Default for TimerActorBuilder {
    fn default() -> Self {
        TimerActorBuilder {
            box_builder: ServerMessageBoxBuilder::new("Timer", 16),
        }
    }
}

impl Builder<TimerActor> for TimerActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<TimerActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> TimerActor {
        let actor_box = self.box_builder.build();
        TimerActor::new(actor_box)
    }
}

impl RuntimeRequestSink for TimerActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl<T: Message + Sync> MessageSink<RequestEnvelope<SetTimeout<T>, Timeout<T>>>
    for TimerActorBuilder
{
    fn get_sender(&self) -> DynSender<RequestEnvelope<SetTimeout<T>, Timeout<T>>> {
        let request_sender = self.box_builder.get_sender();
        Box::new(SetTimeoutSender {
            inner: request_sender,
        })
    }
}

/// A Sender that translates timeout responses on the wire
///
/// This sender receives `Timeout<AnyPayload>` from the `TimerActor`,
/// and translates then forwards these messages to an actor expecting `Timeout<T>`
struct TimeoutSender<T: Message + Sync> {
    inner: Box<dyn Sender<Timeout<T>>>,
}

#[async_trait]
impl<T: Message + Sync> Sender<Timeout<AnyPayload>> for TimeoutSender<T> {
    async fn send(&mut self, message: Timeout<AnyPayload>) -> Result<(), ChannelError> {
        if let Ok(event) = message.event.downcast() {
            self.inner.send(Timeout { event: *event }).await?;
        }
        Ok(())
    }
}

/// A Sender that translates timeout requests on the wire
///
/// This sender receives `RequestEnvelope<SetTimeout<T>, Timeout<T>>` requests from some actor,
/// and translates then forwards these messages to the timer actor
/// which is expecting`RequestEnvelope<Timeout<AnyPayload>, Timeout<AnyPayload>`.
struct SetTimeoutSender {
    inner: DynSender<RequestEnvelope<SetTimeout<AnyPayload>, Timeout<AnyPayload>>>,
}

impl Clone for SetTimeoutSender {
    fn clone(&self) -> Self {
        SetTimeoutSender {
            inner: self.inner.sender_clone(),
        }
    }
}

#[async_trait]
impl<T: Message + Sync> Sender<RequestEnvelope<SetTimeout<T>, Timeout<T>>> for SetTimeoutSender {
    async fn send(
        &mut self,
        RequestEnvelope { request, reply_to }: RequestEnvelope<SetTimeout<T>, Timeout<T>>,
    ) -> Result<(), ChannelError> {
        let duration = request.duration;
        let event: AnyPayload = Box::new(request.event);
        let adapted_reply_to = Box::new(TimeoutSender { inner: reply_to });
        self.inner
            .send(RequestEnvelope {
                request: SetTimeout { duration, event },
                reply_to: adapted_reply_to,
            })
            .await
    }
}
