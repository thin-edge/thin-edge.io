use super::config::C8yMapperConfig;
use super::converter::CumulocityConverter;
use super::dynamic_discovery::process_inotify_events;
use crate::entity_cache::UpdateOutcome;
use crate::service_monitor::is_c8y_bridge_established;
use anyhow::anyhow;
use async_trait::async_trait;
use c8y_http_proxy::handle::C8YHttpProxy;
use std::collections::HashMap;
use std::path::PathBuf;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ClientMessageBox;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sender;
use tedge_actors::Service;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::entity::EntityMetadata;
use tedge_api::entity::EntityType;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::pending_entity_store::RegisteredEntityData;
use tedge_downloader_ext::DownloadRequest;
use tedge_downloader_ext::DownloadResult;
use tedge_file_system_ext::FsWatchEvent;
use tedge_flows::FlowContextHandle;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_uploader_ext::UploadRequest;
use tedge_uploader_ext::UploadResult;
use tedge_utils::file::FileError;
use tedge_utils::paths::PathsError;
use tokio::sync::watch;
use tracing::error;

pub(crate) type CmdId = String;
pub(crate) type IdUploadRequest = (CmdId, UploadRequest);
pub(crate) type IdUploadResult = (CmdId, UploadResult);
pub(crate) type IdDownloadResult = (CmdId, DownloadResult);
pub(crate) type IdDownloadRequest = (CmdId, DownloadRequest);

fan_in_message_type!(C8yMapperInput[MqttMessage, FsWatchEvent] : Debug);

type C8yMapperOutput = MqttMessage;

pub struct C8yMapperActor {
    converter: CumulocityConverter,
    messages: SimpleMessageBox<C8yMapperInput, C8yMapperOutput>,
    mqtt_publisher: LoggingSender<MqttMessage>,
    bridge_status_messages: SimpleMessageBox<MqttMessage, MqttMessage>,
    /// Whether the built-in bridge is subscribed to the topics it relays to the cloud
    ///
    /// Only set when the bridge runs in this process, as it is signalled directly by that bridge
    bridge_subscribed: Option<watch::Receiver<bool>>,
    message_handlers: HashMap<ChannelFilter, Vec<LoggingSender<MqttMessage>>>,
}

/// Bounds the inputs held back while waiting for the built-in bridge to subscribe
///
/// That wait is normally sub-second, so reaching this limit means the bridge cannot subscribe at
/// all — it is unable to connect to the local broker, say. Beyond it, messages are converted as
/// usual: their cloud-bound conversions may be dropped by the broker, which is still better than
/// holding on to every message a device produces.
const MAX_PENDING_INPUTS: usize = 1024;

#[async_trait]
impl Actor for C8yMapperActor {
    fn name(&self) -> &str {
        "CumulocityMapper"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        // On a fresh session, anything published to the cloud topics before the bridge has
        // subscribed to them is dropped, so nothing is converted until the bridge is ready
        let Startup::BridgeReady(pending) = self.wait_until_bridge_is_ready().await? else {
            return Ok(());
        };
        self.publish_init_messages().await?;

        for event in pending {
            self.process_input(event).await?;
        }
        while let Some(event) = self.messages.recv().await {
            self.process_input(event).await?;
        }
        Ok(())
    }
}

/// Outcome of waiting for the bridge before the mapper announces itself to the cloud
enum Startup {
    /// The bridge can relay the messages the mapper publishes, and these inputs arrived while
    /// waiting for it
    BridgeReady(Vec<C8yMapperInput>),

    /// The mapper was shut down while waiting for the bridge
    Interrupted,
}

impl C8yMapperActor {
    pub fn new(
        converter: CumulocityConverter,
        messages: SimpleMessageBox<C8yMapperInput, C8yMapperOutput>,
        mqtt_publisher: LoggingSender<MqttMessage>,
        bridge_status_messages: SimpleMessageBox<MqttMessage, MqttMessage>,
        bridge_subscribed: Option<watch::Receiver<bool>>,
        message_handlers: HashMap<ChannelFilter, Vec<LoggingSender<MqttMessage>>>,
    ) -> Self {
        Self {
            converter,
            messages,
            mqtt_publisher,
            bridge_status_messages,
            bridge_subscribed,
            message_handlers,
        }
    }

    /// Waits until the bridge can relay the mapper's messages to the cloud
    ///
    /// The built-in bridge signals when it is subscribed to the topics it relays; until then
    /// anything published on them is discarded by the broker. Only that subscription is awaited,
    /// not the cloud connection: once the bridge holds the subscription it takes the messages and
    /// forwards them when the cloud is reachable, and the broker bounds how many it holds
    /// meanwhile. Inputs arriving before then are held back rather than converted, but they are
    /// still received, as a full input channel would stall the MQTT actor for every other actor in
    /// this process.
    ///
    /// This is awaited once, not on every reconnection: the bridge keeps its session on the local
    /// broker, so from then on the broker queues what it cannot deliver instead of discarding it.
    ///
    /// Mosquitto's bridge subscribes for itself before the mapper starts, so it only has to be
    /// running: it queues the messages the mapper publishes until the cloud is reachable.
    async fn wait_until_bridge_is_ready(&mut self) -> Result<Startup, RuntimeError> {
        let Some(mut bridge_subscribed) = self.bridge_subscribed.take() else {
            while let Some(message) = self.bridge_status_messages.recv().await {
                if is_c8y_bridge_established(
                    &message,
                    &self.converter.config.mqtt_schema,
                    &self.converter.config.bridge_health_topic,
                ) {
                    break;
                }
            }
            return Ok(Startup::BridgeReady(vec![]));
        };

        let mut pending = Vec::new();
        let mut overflowed = false;
        loop {
            if *bridge_subscribed.borrow_and_update() {
                return Ok(Startup::BridgeReady(pending));
            }
            tokio::select! {
                subscribed = bridge_subscribed.changed() => {
                    // The bridge has stopped, so nothing will signal this again
                    if subscribed.is_err() {
                        return Ok(Startup::BridgeReady(pending));
                    }
                }
                event = self.messages.recv() => match event {
                    None => return Ok(Startup::Interrupted),
                    Some(event) if pending.len() < MAX_PENDING_INPUTS => pending.push(event),
                    Some(event) => {
                        if !overflowed {
                            overflowed = true;
                            error!(
                                "The bridge is still not subscribed after {MAX_PENDING_INPUTS} messages: \
                                 the messages received from now on are converted, but the cloud may not receive them"
                            );
                        }
                        self.process_input(event).await?
                    }
                },
            }
        }
    }

    /// Publishes the messages the mapper sends once the bridge is ready: the operations it
    /// supports and a request for any operation pending in the cloud
    async fn publish_init_messages(&mut self) -> Result<(), RuntimeError> {
        let init_messages = self.converter.init_messages();
        for init_message in init_messages.into_iter() {
            self.mqtt_publisher.send(init_message).await?;
        }
        Ok(())
    }

    async fn process_input(&mut self, event: C8yMapperInput) -> Result<(), RuntimeError> {
        match event {
            C8yMapperInput::MqttMessage(message) => {
                self.process_mqtt_message(message).await?;
            }
            C8yMapperInput::FsWatchEvent(event) => {
                self.process_file_watch_event(event).await?;
            }
        }
        Ok(())
    }

    /// Processing an incoming message involves the following steps, if the message follows MQTT topic scheme v1:
    /// 1. Try to register the source entity and any of its cached pending children for the incoming message
    /// 2. For each entity that got registered in the previous step
    ///    1. Convert and publish that registration message
    ///    2. Publish that registration messages to any message handlers interested in that message type
    ///    3. Convert and publish all the cached data messages of that entity to the cloud
    ///    4. Publish those data messages also to any message handlers interested in those message types
    /// 3. Once all the required entities and their cached data is processed, process the incoming message itself
    ///    1. Convert and publish that message to the cloud
    ///    2. Publish that message to any message handlers interested in its message type
    ///
    /// If the message follows the legacy topic scheme v0, the data message is simply converted the old way.
    async fn process_mqtt_message(&mut self, message: MqttMessage) -> Result<(), RuntimeError> {
        // If incoming message follows MQTT topic scheme v1
        if let Ok((_, channel)) = self.converter.mqtt_schema.entity_channel_of(&message.topic) {
            if channel.is_entity_metadata() {
                // If the message is an entity registration message, process it
                match self
                    .converter
                    .process_entity_metadata_message(&message)
                    .await
                {
                    Ok(outcome) => match outcome {
                        UpdateOutcome::Inserted(registered_entities) => {
                            self.process_registered_entities(registered_entities)
                                .await?
                        }
                        UpdateOutcome::Updated(updated_entity, old_entity) => {
                            self.process_entity_update(*updated_entity, *old_entity)
                                .await?
                        }
                        UpdateOutcome::Deleted | UpdateOutcome::Unchanged => (),
                    },
                    Err(err) => {
                        self.mqtt_publisher
                            .send(self.converter.new_error_message(err))
                            .await?;
                        return Ok(());
                    }
                }
            } else {
                self.process_message(message).await?;
            }
        } else {
            self.convert_and_publish(&message).await?;
        }

        Ok(())
    }

    /// Process a list of registered entities with their cached data.
    /// For each entity its registration message is converted and published to the cloud
    /// and any of the interested message handlers for that type,
    /// followed by repeating the same for its cached data messages.
    pub(crate) async fn process_registered_entities(
        &mut self,
        pending_entities: Vec<RegisteredEntityData>,
    ) -> Result<(), RuntimeError> {
        for pending_entity in pending_entities {
            let mut reg_message = pending_entity.reg_message;
            self.converter.append_id_if_not_given(&mut reg_message);
            let reg_message = reg_message.to_mqtt_message(&self.converter.mqtt_schema);
            self.process_message(reg_message).await?;

            // Convert and publish cached data messages
            for pending_data_message in pending_entity.data_messages {
                // TODO: Is this still useful?
                //       MEA messages are no more cached by the c8y converter but by the flows
                self.process_message(pending_data_message).await?;
            }
        }

        Ok(())
    }

    pub(crate) async fn process_entity_update(
        &mut self,
        updated_entity: EntityMetadata,
        old_entity: EntityMetadata,
    ) -> Result<(), RuntimeError> {
        if updated_entity.parent != old_entity.parent {
            let entity = self
                .converter
                .entity_cache
                .get(&updated_entity.topic_id)
                .expect("Entity should be present in the cache");
            let child_xid = &entity.external_id;
            let old_parent_xid = self
                .converter
                .entity_cache
                .try_get_external_id(&old_entity.parent.unwrap())
                .expect("Device external id should be present");
            let new_parent_xid = self
                .converter
                .entity_cache
                .try_get_external_id(&updated_entity.parent.clone().unwrap())
                .expect("Device external id should be present");

            let res = match entity.r#type() {
                EntityType::MainDevice => {
                    Err(anyhow!("Main device parent update is not supported").into())
                }
                EntityType::ChildDevice => {
                    self.converter
                        .http_proxy
                        .update_child_device_parent(
                            child_xid.as_ref(),
                            old_parent_xid.as_ref(),
                            new_parent_xid.as_ref(),
                        )
                        .await
                }
                EntityType::Service => {
                    self.converter
                        .http_proxy
                        .update_child_addition_parent(
                            child_xid.as_ref(),
                            old_parent_xid.as_ref(),
                            new_parent_xid.as_ref(),
                        )
                        .await
                }
            };

            if let Err(err) = res {
                self.mqtt_publisher
                    .send(self.converter.new_error_message(err))
                    .await?;
            }
        }

        let entity_reg_msg: EntityRegistrationMessage =
            EntityRegistrationMessage::from(&updated_entity);
        let message = entity_reg_msg.to_mqtt_message(&self.converter.mqtt_schema);
        // Send the registration message to all subscribed handlers
        self.publish_message_to_subscribed_handles(&Channel::EntityMetadata, message)
            .await?;

        Ok(())
    }

    //  Process an MQTT message by converting and publishing it to the cloud
    /// and any of the message handlers interested in its type.
    async fn process_message(&mut self, message: MqttMessage) -> Result<(), RuntimeError> {
        if let Ok((_, channel)) = self.converter.mqtt_schema.entity_channel_of(&message.topic) {
            self.convert_and_publish(&message).await?;
            self.publish_message_to_subscribed_handles(&channel, message)
                .await?;
        }

        Ok(())
    }

    async fn convert_and_publish(&mut self, message: &MqttMessage) -> Result<(), RuntimeError> {
        // Convert and publish the incoming data message
        let converted_messages = self.converter.convert(message).await;
        self.publish_messages(converted_messages).await?;

        Ok(())
    }

    async fn publish_message_to_subscribed_handles(
        &mut self,
        channel: &Channel,
        message: MqttMessage,
    ) -> Result<(), RuntimeError> {
        // Send the registration message to all subscribed handlers
        if let Some(message_handler) = self.message_handlers.get_mut(&channel.into()) {
            for sender in message_handler {
                sender.send(message.clone()).await?;
            }
        }
        Ok(())
    }

    async fn publish_messages(&mut self, messages: Vec<MqttMessage>) -> Result<(), RuntimeError> {
        for message in messages.into_iter() {
            self.mqtt_publisher.send(message).await?;
        }
        Ok(())
    }

    async fn process_file_watch_event(
        &mut self,
        file_event: FsWatchEvent,
    ) -> Result<(), RuntimeError> {
        match file_event.clone() {
            FsWatchEvent::FileCreated(path)
            | FsWatchEvent::FileDeleted(path)
            | FsWatchEvent::Modified(path) => {
                // Process inotify events only for the main device at the root operations directory
                // directly under /etc/tedge/operations/c8y
                let ops_dir = self.converter.config.ops_dir.path().as_std_path();
                if path.parent() == Some(ops_dir) {
                    match process_inotify_events(ops_dir, &path, file_event) {
                        Ok(Some(discovered_ops)) => {
                            if let Some(update_message) = self
                                .converter
                                .process_operation_update_message(discovered_ops)
                            {
                                self.mqtt_publisher.send(update_message).await?;
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            eprintln!("Processing inotify event failed due to {}", e);
                        }
                    }
                }
            }
            FsWatchEvent::DirectoryCreated(_) | FsWatchEvent::DirectoryDeleted(_) => {}
        }

        Ok(())
    }
}

pub struct C8yMapperBuilder {
    pub(crate) config: C8yMapperConfig,
    box_builder: SimpleMessageBoxBuilder<C8yMapperInput, C8yMapperOutput>,
    mqtt_publisher: DynSender<MqttMessage>,
    http_proxy: C8YHttpProxy,
    http_client: ClientMessageBox<HttpRequest, HttpResult>,
    downloader: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
    uploader: ClientMessageBox<IdUploadRequest, IdUploadResult>,
    bridge_monitor_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
    bridge_subscribed: Option<watch::Receiver<bool>>,
    message_handlers: HashMap<ChannelFilter, Vec<LoggingSender<MqttMessage>>>,
    flow_context: Option<FlowContextHandle>,
}

impl C8yMapperBuilder {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        config: C8yMapperConfig,
        mqtt: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
        http: &mut impl Service<HttpRequest, HttpResult>,
        uploader: &mut impl Service<IdUploadRequest, IdUploadResult>,
        downloader: &mut impl Service<IdDownloadRequest, IdDownloadResult>,
        fs_watcher: &mut impl MessageSource<FsWatchEvent, PathBuf>,
        service_monitor: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
    ) -> Result<Self, FileError> {
        let box_builder: SimpleMessageBoxBuilder<C8yMapperInput, C8yMapperOutput> =
            SimpleMessageBoxBuilder::new("CumulocityMapper", 16);

        let mqtt_publisher = mqtt.get_sender();
        mqtt.connect_sink(config.topics.clone(), &box_builder.get_sender());

        let http_proxy = C8YHttpProxy::new(&config, http);
        let http_client = ClientMessageBox::new(http);

        let downloader = ClientMessageBox::new(downloader);
        let uploader = ClientMessageBox::new(uploader);

        fs_watcher.connect_sink(
            config.ops_dir.path().as_std_path().to_path_buf(),
            &box_builder.get_sender(),
        );

        let bridge_monitor_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
            SimpleMessageBoxBuilder::new("ServiceMonitor", 1);

        service_monitor.connect_sink(
            config.bridge_health_topic.clone().into(),
            &bridge_monitor_builder,
        );

        let message_handlers = HashMap::new();

        Ok(Self {
            config,
            box_builder,
            mqtt_publisher,
            http_proxy,
            http_client,
            uploader,
            downloader,
            bridge_monitor_builder,
            bridge_subscribed: None,
            message_handlers,
            flow_context: None,
        })
    }

    pub fn set_flow_context(&mut self, flow_context: FlowContextHandle) {
        self.flow_context = Some(flow_context);
    }

    /// Tells the mapper when the built-in bridge is subscribed to the topics it relays
    ///
    /// This has to be set whenever the bridge runs in this process, as the mapper publishes
    /// nothing to the cloud until the bridge holds those subscriptions
    pub fn set_bridge_subscribed(&mut self, bridge_subscribed: watch::Receiver<bool>) {
        self.bridge_subscribed = Some(bridge_subscribed);
    }

    pub async fn init(config: &C8yMapperConfig) -> Result<(), PathsError> {
        // Create c8y operations directory
        config.ops_dir.ensure().await?;
        Ok(())
    }
}

impl RuntimeRequestSink for C8yMapperBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl MessageSource<MqttMessage, Vec<ChannelFilter>> for C8yMapperBuilder {
    fn connect_sink(&mut self, config: Vec<ChannelFilter>, peer: &impl MessageSink<MqttMessage>) {
        let sender = LoggingSender::new("Mapper MQTT".into(), peer.get_sender());
        for channel in config {
            self.message_handlers
                .entry(channel)
                .or_default()
                .push(sender.clone());
        }
    }
}

impl MessageSink<MqttMessage> for C8yMapperBuilder {
    fn get_sender(&self) -> DynSender<MqttMessage> {
        self.mqtt_publisher.sender_clone()
    }
}

impl Builder<C8yMapperActor> for C8yMapperBuilder {
    type Error = RuntimeError;

    fn try_build(self) -> Result<C8yMapperActor, Self::Error> {
        if self.config.bridge_in_mapper && self.bridge_subscribed.is_none() {
            return Err(RuntimeError::ActorError(
                anyhow!("the built-in bridge runs in the mapper, but the mapper was not given its subscription state").into(),
            ));
        }

        let mqtt_publisher = LoggingSender::new("C8yMapper => Mqtt".into(), self.mqtt_publisher);

        let converter = CumulocityConverter::new(
            self.config,
            mqtt_publisher.clone(),
            self.http_proxy,
            self.uploader,
            self.downloader,
            self.http_client,
            self.flow_context.unwrap_or_default(),
        )
        .map_err(|err| RuntimeError::ActorError(Box::new(err)))?;

        let message_box = self.box_builder.build();
        let bridge_monitor_box = self.bridge_monitor_builder.build();

        Ok(C8yMapperActor::new(
            converter,
            message_box,
            mqtt_publisher,
            bridge_monitor_box,
            self.bridge_subscribed,
            self.message_handlers,
        ))
    }
}
