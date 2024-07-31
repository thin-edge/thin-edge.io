use async_trait::async_trait;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_auth_proxy::url::ProxyUrlGenerator;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::futures::select;
use tedge_actors::futures::FutureExt;
use tedge_actors::futures::StreamExt;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ClientMessageBox;
use tedge_actors::CloneSender;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sender;
use tedge_actors::Service;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::entity_store::EntityMetadata;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_mqtt_ext::MqttMessage;

use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::actor::IdUploadRequest;
use crate::actor::IdUploadResult;
use crate::actor::PublishMessage;
use crate::config::C8yMapperConfig;

use super::handlers::OperationMessage;
use super::EntityTarget;
use super::OperationHandler;

pub struct OperationHandlerActor {
    messages: SimpleMessageBox<OperationMessage, PublishMessage>,
    operation_handler: OperationHandler,
    operation_handler_output: mpsc::Receiver<MqttMessage>,
}

#[async_trait]
impl Actor for OperationHandlerActor {
    fn name(&self) -> &str {
        "OperationHandler"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let (mut sender, mut receiver) = self.messages.into_split();

        let mut operation_handler = self.operation_handler;

        let mut operation_handler_output = self.operation_handler_output;

        loop {
            select! {
                output_message = operation_handler_output.next() => {
                    let message = output_message.unwrap();

                    let message = PublishMessage(message);
                    sender.send(message).await.unwrap();
                }
                input_message = receiver.recv().fuse() => {
                    if input_message.is_none() {
                        break;
                    }
                    let op = input_message.unwrap();
                    let entity = op.entity;
                    let message = op.message;
                    operation_handler.handle(entity, message).await;
                }
            }
        }

        Ok(())
    }
}

pub struct OperationHandlerActorBuilder {
    operation_handler: OperationHandler,
    box_builder: SimpleMessageBoxBuilder<OperationMessage, PublishMessage>,
    operation_handler_output: mpsc::Receiver<MqttMessage>,
}

impl OperationHandlerActorBuilder {
    pub fn new(
        c8y_mapper_config: &C8yMapperConfig,

        mqtt: &mut (impl MessageSource<(MqttMessage, EntityMetadata), Vec<ChannelFilter>>
                  + MessageSink<PublishMessage>),
        uploader: &mut impl Service<IdUploadRequest, IdUploadResult>,
        downloader: &mut impl Service<IdDownloadRequest, IdDownloadResult>,

        http: &mut impl Service<C8YRestRequest, C8YRestResult>,
        auth_proxy: ProxyUrlGenerator,
    ) -> Self {
        // if there are any outgoing MQTT messages, send them immediately
        let (operation_handler_sender, operation_handler_receiver) = mpsc::channel(10);

        let uploader = ClientMessageBox::new(uploader);
        let downloader = ClientMessageBox::new(downloader);

        let c8y_http_proxy = C8YHttpProxy::new(http);

        let operation_handler = OperationHandler::new(
            c8y_mapper_config,
            downloader,
            uploader,
            operation_handler_sender.sender_clone(),
            c8y_http_proxy,
            auth_proxy,
        );

        // TODO(marcel): discarding EntityFilter portion because C8yMapperActor doesn't support it, perhaps it should
        let config = OperationHandler::topic_filter(&c8y_mapper_config.capabilities)
            .into_iter()
            .map(|f| f.1)
            .collect::<Vec<_>>();

        let mut box_builder: SimpleMessageBoxBuilder<OperationMessage, PublishMessage> =
            SimpleMessageBoxBuilder::new("OperationHandlerActor", 10);

        box_builder.connect_mapped_source(
            config,
            mqtt,
            Self::mqtt_message_parser(c8y_mapper_config),
        );

        mqtt.connect_source(NoConfig, &mut box_builder);

        Self {
            operation_handler,
            box_builder,
            operation_handler_output: operation_handler_receiver,
        }
    }

    fn mqtt_message_parser(
        config: &C8yMapperConfig,
    ) -> impl Fn((MqttMessage, EntityMetadata)) -> Option<OperationMessage> {
        let mqtt_schema = config.mqtt_schema.clone();
        let prefix = config.c8y_prefix.clone();

        move |(message, metadata)| {
            let (_, channel) = mqtt_schema.entity_channel_of(&message.topic).unwrap();

            // if not Command, then CommandMetadata
            let Channel::Command { operation, cmd_id } = channel else {
                return None;
            };

            let smartrest_publish_topic = C8yTopic::smartrest_response_topic(&metadata, &prefix)
                .expect("should create a valid topic");

            Some(OperationMessage {
                message,
                operation,
                cmd_id: cmd_id.into(),
                entity: EntityTarget {
                    topic_id: metadata.topic_id,
                    external_id: metadata.external_id,
                    smartrest_publish_topic,
                },
            })
        }
    }
}

impl Builder<OperationHandlerActor> for OperationHandlerActorBuilder {
    type Error = std::convert::Infallible;

    fn try_build(self) -> Result<OperationHandlerActor, Self::Error> {
        Ok(OperationHandlerActor {
            operation_handler: self.operation_handler,
            messages: self.box_builder.build(),
            operation_handler_output: self.operation_handler_output,
        })
    }
}

impl RuntimeRequestSink for OperationHandlerActorBuilder {
    fn get_signal_sender(&self) -> tedge_actors::DynSender<tedge_actors::RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}
