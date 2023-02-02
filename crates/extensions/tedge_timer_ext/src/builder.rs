use crate::actor::TimerActor;
use crate::actor::TimerId;
use crate::SetTimeout;
use crate::Timeout;
use async_trait::async_trait;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::Message;
use tedge_actors::MessageBoxPlug;
use tedge_actors::MessageBoxSocket;
use tedge_actors::NoConfig;
use tedge_actors::Sender;
use tedge_actors::ServiceMessageBoxBuilder;
use tokio::sync::Mutex;

pub struct TimerActorBuilder {
    box_builder: ServiceMessageBoxBuilder<SetTimeout<TimerId>, Timeout<TimerId>>,
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

impl<T: Message> MessageBoxSocket<SetTimeout<T>, Timeout<T>, NoConfig> for TimerActorBuilder {
    fn connect_with(
        &mut self,
        peer: &mut impl MessageBoxPlug<SetTimeout<T>, Timeout<T>>,
        config: NoConfig,
    ) {
        let mut adapter = GenericTimerAdapter::new(peer);
        self.box_builder.connect_with(&mut adapter, config);
    }
}

/// A message adapter used by actors to send timer requests with a generic payload `SetTimeout<T>`
/// and to receive accordingly timer responses with a generic payload `Timeout<T>`,
/// while the timer actor only handles `usize` payloads.
///
/// This adapter uses a cache from assigned `TimerId` to actual data of type `T`.
/// This cache is populated for each received `SetTimeout<T>` request
/// on translation into a `SetTimeout<TimerId>`; and cleaned when the `Timeout<TimerId>` is received
/// and translated back into a `Timeout<TimerId>`.
///
/// Technically, the cache needs to be wrapped behind an `Arc<Mutex< ... >>`,
/// because being accessed by two independent actors: the caller and the timer actors.
///
/// - The cache is accessed by the caller through a `GenericTimerRequestAdapter<T: Message>`,
///   used as a `Sender<SetTimeout<T>>, and translating requests from `T` to `TimerId`.
///
/// - The cache is accessed by the timer actor through a `GenericTimerResponseAdapter<T: Message>`,
///   used as a `Sender<Timeout<TimerId>>, and translating responses from `TimerId` to `T`.
struct GenericTimerAdapter<'a, T: Message, Peer> {
    cache: Arc<Mutex<GenericTimerCache<T>>>,
    peer: &'a mut Peer,
}

impl<'a, T: Message, Peer> GenericTimerAdapter<'a, T, Peer> {
    fn new(peer: &'a mut Peer) -> Self {
        let cache = Arc::new(Mutex::new(GenericTimerCache::new()));

        GenericTimerAdapter { cache, peer }
    }
}

impl<'a, T: Message, Peer> MessageBoxPlug<SetTimeout<TimerId>, Timeout<TimerId>>
    for GenericTimerAdapter<'a, T, Peer>
where
    Peer: MessageBoxPlug<SetTimeout<T>, Timeout<T>>,
{
    fn set_request_sender(&mut self, request_sender: DynSender<SetTimeout<TimerId>>) {
        self.peer
            .set_request_sender(Box::new(GenericTimerRequestAdapter {
                request_sender,
                cache: Arc::clone(&self.cache),
            }))
    }

    fn get_response_sender(&self) -> DynSender<Timeout<TimerId>> {
        Box::new(GenericTimerResponseAdapter {
            response_sender: self.peer.get_response_sender(),
            cache: Arc::clone(&self.cache),
        })
    }
}

/// A cache of generic timer events
///
/// An entry is created for each timer request,
/// and removed when the associated response is back.
struct GenericTimerCache<T: Message> {
    next_id: TimerId,
    cache: HashMap<TimerId, T>,
}

impl<T: Message> GenericTimerCache<T> {
    fn new() -> Self {
        GenericTimerCache {
            next_id: 0,
            cache: HashMap::new(),
        }
    }

    fn translate_request(&mut self, request: SetTimeout<T>) -> SetTimeout<TimerId> {
        let timer_id = self.next_id;

        self.next_id += 1;
        self.cache.insert(timer_id, request.event);

        SetTimeout {
            duration: request.duration,
            event: timer_id,
        }
    }

    fn translate_response(&mut self, response: Timeout<TimerId>) -> Option<Timeout<T>> {
        self.cache
            .remove(&response.event)
            .map(|event| Timeout { event })
    }
}

/// A Sender adapter given the caller to send its requests
struct GenericTimerRequestAdapter<T: Message> {
    request_sender: DynSender<SetTimeout<TimerId>>,
    cache: Arc<Mutex<GenericTimerCache<T>>>,
}

#[async_trait]
impl<T: Message> Sender<SetTimeout<T>> for GenericTimerRequestAdapter<T> {
    async fn send(&mut self, request: SetTimeout<T>) -> Result<(), ChannelError> {
        let translated_request = {
            let mut cache = self.cache.lock().await;
            cache.translate_request(request)
        };
        self.request_sender.send(translated_request).await
    }

    fn sender_clone(&self) -> DynSender<SetTimeout<T>> {
        Box::new(GenericTimerRequestAdapter {
            request_sender: self.request_sender.sender_clone(),
            cache: Arc::clone(&self.cache),
        })
    }

    fn close_sender(&mut self) {
        self.request_sender.as_mut().close_sender();
    }
}

/// A Sender adapter given the timer actor to send its responses
struct GenericTimerResponseAdapter<T: Message> {
    response_sender: DynSender<Timeout<T>>,
    cache: Arc<Mutex<GenericTimerCache<T>>>,
}

#[async_trait]
impl<T: Message> Sender<Timeout<TimerId>> for GenericTimerResponseAdapter<T> {
    async fn send(&mut self, response: Timeout<TimerId>) -> Result<(), ChannelError> {
        let translated_response = {
            let mut cache = self.cache.lock().await;
            cache.translate_response(response)
        };
        if let Some(translated_response) = translated_response {
            self.response_sender.send(translated_response).await?
        }
        Ok(())
    }

    fn sender_clone(&self) -> DynSender<Timeout<TimerId>> {
        Box::new(GenericTimerResponseAdapter {
            response_sender: self.response_sender.sender_clone(),
            cache: Arc::clone(&self.cache),
        })
    }

    fn close_sender(&mut self) {
        self.response_sender.as_mut().close_sender();
    }
}
