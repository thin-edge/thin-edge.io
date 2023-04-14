use crate::collectd::CollectdMessage;
use async_trait::async_trait;
use log::error;
use std::convert::Infallible;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sender;
use tedge_actors::ServiceConsumer;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;

/// An actor that collects measurements from collectd over MQTT
pub struct CollectdActor {
    messages: SimpleMessageBox<MqttMessage, CollectdMessage>,
}

#[async_trait]
impl Actor for CollectdActor {
    fn name(&self) -> &str {
        "collectd"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        while let Some(message) = self.messages.recv().await {
            match CollectdMessage::parse_from(&message) {
                Ok(collectd_message) => {
                    for msg in collectd_message {
                        self.messages.send(msg).await?
                    }
                }
                Err(err) => {
                    error!("Error while decoding a collectd message: {}", err);
                }
            }
        }
        Ok(())
    }
}

pub struct CollectdActorBuilder {
    topics: TopicFilter,
    message_box: SimpleMessageBoxBuilder<MqttMessage, CollectdMessage>,
}

impl CollectdActorBuilder {
    pub fn new(topics: TopicFilter) -> Self {
        CollectdActorBuilder {
            topics,
            message_box: SimpleMessageBoxBuilder::new("Collectd", 16),
        }
    }

    pub fn add_input(&mut self, source: &mut impl MessageSource<MqttMessage, TopicFilter>) {
        source.register_peer(self.topics.clone(), self.message_box.get_sender())
    }
}

impl ServiceConsumer<MqttMessage, MqttMessage, TopicFilter> for CollectdActorBuilder {
    fn get_config(&self) -> TopicFilter {
        self.topics.clone()
    }

    fn set_request_sender(&mut self, _request_sender: DynSender<MqttMessage>) {
        // this actor publishes no messages over MQTT
    }

    fn get_response_sender(&self) -> DynSender<MqttMessage> {
        self.message_box.get_response_sender()
    }
}

impl RuntimeRequestSink for CollectdActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.message_box.get_signal_sender()
    }
}

impl MessageSource<CollectdMessage, NoConfig> for CollectdActorBuilder {
    fn register_peer(&mut self, config: NoConfig, sender: DynSender<CollectdMessage>) {
        self.message_box.register_peer(config, sender)
    }
}

impl Builder<CollectdActor> for CollectdActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<CollectdActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> CollectdActor {
        CollectdActor {
            messages: self.message_box.build(),
        }
    }
}
