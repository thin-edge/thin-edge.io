use super::actor::EntityManagerActor;
use futures::channel::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use tedge_actors::Builder;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::LoggingReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_api::EntityStore;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;

pub struct EntityManagerActorBuilder {
    input_receiver: LoggingReceiver<MqttMessage>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
    entity_store: Arc<Mutex<EntityStore>>,
}

impl EntityManagerActorBuilder {
    pub fn try_new(
        mqtt_actor: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
        entity_store: Arc<Mutex<EntityStore>>,
    ) -> Self {
        let (input_sender, input_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let input_receiver =
            LoggingReceiver::new("Entity-Manager".into(), input_receiver, signal_receiver);

        let input_sender: DynSender<MqttMessage> = input_sender.sender_clone();
        mqtt_actor.connect_sink(Self::subscriptions(), &input_sender);

        Self {
            input_receiver,
            signal_sender,
            entity_store,
        }
    }

    fn subscriptions() -> TopicFilter {
        vec!["te/+/+/+/+"].try_into().unwrap()
    }
}

impl RuntimeRequestSink for EntityManagerActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.signal_sender.clone())
    }
}

impl Builder<EntityManagerActor> for EntityManagerActorBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<EntityManagerActor, Self::Error> {
        Ok(EntityManagerActor::new(
            self.input_receiver,
            self.entity_store,
        ))
    }
}
