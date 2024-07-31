use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Builder;
use tedge_actors::ClientMessageBox;
use tedge_actors::CloneSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Service;
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

use super::actor::OperationHandlerActor;
use super::actor::RunningOperations;
use super::handler::OperationHandlerConfig;
use super::handlers::OperationMessage;
use super::EntityTarget;
use super::OperationHandler;

pub struct OperationHandlerBuilder {
    operation_handler: OperationHandler,
    box_builder: SimpleMessageBoxBuilder<OperationMessage, PublishMessage>,
}

impl OperationHandlerBuilder {
    pub fn new(
        config: OperationHandlerConfig,

        mqtt: &mut (impl MessageSource<(MqttMessage, EntityMetadata), Vec<ChannelFilter>>
                  + MessageSink<PublishMessage>),
        uploader: &mut impl Service<IdUploadRequest, IdUploadResult>,
        downloader: &mut impl Service<IdDownloadRequest, IdDownloadResult>,

        http: &mut impl Service<C8YRestRequest, C8YRestResult>,
    ) -> Self {
        // if there are any outgoing MQTT messages, send them immediately
        let (operation_handler_sender, _) = mpsc::channel::<MqttMessage>(10);

        let uploader = ClientMessageBox::new(uploader);
        let downloader = ClientMessageBox::new(downloader);

        let c8y_http_proxy = C8YHttpProxy::new(http);

        // TODO(marcel): discarding EntityFilter portion because C8yMapperActor doesn't support it, perhaps it should
        let channel_filter = OperationHandler::topic_filter(&config.capabilities)
            .into_iter()
            .map(|f| f.1)
            .collect::<Vec<_>>();

        let mut box_builder: SimpleMessageBoxBuilder<OperationMessage, PublishMessage> =
            SimpleMessageBoxBuilder::new("OperationHandlerActor", 10);

        box_builder.connect_mapped_source(channel_filter, mqtt, Self::mqtt_message_parser(&config));

        mqtt.connect_source(NoConfig, &mut box_builder);

        let operation_handler = OperationHandler::new(
            config,
            downloader,
            uploader,
            operation_handler_sender.sender_clone(),
            c8y_http_proxy,
        );

        Self {
            operation_handler,
            box_builder,
        }
    }

    fn mqtt_message_parser(
        config: &OperationHandlerConfig,
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

impl Builder<OperationHandlerActor> for OperationHandlerBuilder {
    type Error = std::convert::Infallible;

    fn try_build(self) -> Result<OperationHandlerActor, Self::Error> {
        let context = self.operation_handler.context.clone();
        Ok(OperationHandlerActor {
            operation_handler: self.operation_handler,
            messages: self.box_builder.build(),
            running_operations: RunningOperations {
                current_statuses: Default::default(),
                context,
            },
        })
    }
}

impl RuntimeRequestSink for OperationHandlerBuilder {
    fn get_signal_sender(&self) -> tedge_actors::DynSender<tedge_actors::RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}
