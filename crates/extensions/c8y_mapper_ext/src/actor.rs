use super::config::C8yMapperConfig;
use super::converter::CumulocityConverter;
use super::dynamic_discovery::process_inotify_events;
use crate::service_monitor::is_c8y_bridge_established;
use async_trait::async_trait;
use c8y_http_proxy::handle::C8YHttpProxy;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
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
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::pending_entity_store::RegisteredEntityData;
use tedge_downloader_ext::DownloadRequest;
use tedge_downloader_ext::DownloadResult;
use tedge_file_system_ext::FsWatchEvent;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;
use tedge_uploader_ext::UploadRequest;
use tedge_uploader_ext::UploadResult;
use tedge_utils::file_async::create_directory_with_defaults;
use tedge_utils::file_async::FileError;

const SYNC_WINDOW: Duration = Duration::from_secs(3);

pub type SyncStart = SetTimeout<()>;
pub type SyncComplete = Timeout<()>;

pub(crate) type CmdId = String;
pub(crate) type IdUploadRequest = (CmdId, UploadRequest);
pub(crate) type IdUploadResult = (CmdId, UploadResult);
pub(crate) type IdDownloadResult = (CmdId, DownloadResult);
pub(crate) type IdDownloadRequest = (CmdId, DownloadRequest);

fan_in_message_type!(C8yMapperInput[MqttMessage, FsWatchEvent, SyncComplete] : Debug);

type C8yMapperOutput = MqttMessage;

pub struct C8yMapperActor {
    converter: CumulocityConverter,
    messages: SimpleMessageBox<C8yMapperInput, C8yMapperOutput>,
    mqtt_publisher: LoggingSender<MqttMessage>,
    timer_sender: LoggingSender<SyncStart>,
    bridge_status_messages: SimpleMessageBox<MqttMessage, MqttMessage>,
    message_handlers: HashMap<ChannelFilter, Vec<LoggingSender<MqttMessage>>>,
}

#[async_trait]
impl Actor for C8yMapperActor {
    fn name(&self) -> &str {
        "CumulocityMapper"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        if !self.converter.config.bridge_in_mapper {
            // Wait till the c8y bridge is established
            while let Some(message) = self.bridge_status_messages.recv().await {
                if is_c8y_bridge_established(
                    &message,
                    &self.converter.config.mqtt_schema,
                    &self.converter.config.bridge_health_topic,
                ) {
                    break;
                }
            }
        }

        let init_messages = self.converter.init_messages();
        for init_message in init_messages.into_iter() {
            self.mqtt_publisher.send(init_message).await?;
        }

        // Start the sync phase
        self.timer_sender
            .send(SyncStart::new(SYNC_WINDOW, ()))
            .await?;

        while let Some(event) = self.messages.recv().await {
            match event {
                C8yMapperInput::MqttMessage(message) => {
                    self.process_mqtt_message(message).await?;
                }
                C8yMapperInput::FsWatchEvent(event) => {
                    self.process_file_watch_event(event).await?;
                }
                C8yMapperInput::SyncComplete(_) => {
                    self.process_sync_timeout().await?;
                }
            }
        }
        Ok(())
    }
}

impl C8yMapperActor {
    pub fn new(
        converter: CumulocityConverter,
        messages: SimpleMessageBox<C8yMapperInput, C8yMapperOutput>,
        mqtt_publisher: LoggingSender<MqttMessage>,
        timer_sender: LoggingSender<SyncStart>,
        bridge_status_messages: SimpleMessageBox<MqttMessage, MqttMessage>,
        message_handlers: HashMap<ChannelFilter, Vec<LoggingSender<MqttMessage>>>,
    ) -> Self {
        Self {
            converter,
            messages,
            mqtt_publisher,
            timer_sender,
            bridge_status_messages,
            message_handlers,
        }
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
            match self.converter.try_register_source_entities(&message).await {
                Ok(pending_entities) => {
                    self.process_registered_entities(pending_entities).await?;
                }
                Err(err) => {
                    self.mqtt_publisher
                        .send(self.converter.new_error_message(err))
                        .await?;
                    return Ok(());
                }
            }

            if !channel.is_entity_metadata() {
                self.process_message(message.clone()).await?;
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
            self.process_registration_message(pending_entity.reg_message)
                .await?;

            // Convert and publish cached data messages
            for pending_data_message in pending_entity.data_messages {
                self.process_message(pending_data_message).await?;
            }
        }

        Ok(())
    }

    async fn process_registration_message(
        &mut self,
        mut message: EntityRegistrationMessage,
    ) -> Result<(), RuntimeError> {
        self.converter.append_id_if_not_given(&mut message);
        // Convert and publish the registration message
        let reg_messages = self.converter.convert_entity_registration_message(&message);
        self.publish_messages(reg_messages).await?;

        // Send the registration message to all subscribed handlers
        self.publish_message_to_subscribed_handles(
            &Channel::EntityMetadata,
            message
                .clone()
                .to_mqtt_message(&self.converter.mqtt_schema)
                .clone(),
        )
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
                if path.parent() == Some(self.converter.config.ops_dir.as_std_path()) {
                    match process_inotify_events(
                        self.converter.config.ops_dir.as_std_path(),
                        &path,
                        file_event,
                    ) {
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

    pub async fn process_sync_timeout(&mut self) -> Result<(), RuntimeError> {
        // Once the sync phase is complete, retrieve all sync messages from the converter and process them
        let sync_messages = self.converter.sync_messages();
        for message in sync_messages {
            self.process_mqtt_message(message).await?;
        }

        Ok(())
    }
}

pub struct C8yMapperBuilder {
    config: C8yMapperConfig,
    box_builder: SimpleMessageBoxBuilder<C8yMapperInput, C8yMapperOutput>,
    mqtt_publisher: DynSender<MqttMessage>,
    http_proxy: C8YHttpProxy,
    timer_sender: DynSender<SyncStart>,
    downloader: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
    uploader: ClientMessageBox<IdUploadRequest, IdUploadResult>,
    bridge_monitor_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
    message_handlers: HashMap<ChannelFilter, Vec<LoggingSender<MqttMessage>>>,
}

impl C8yMapperBuilder {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        config: C8yMapperConfig,
        mqtt: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
        http: &mut impl Service<HttpRequest, HttpResult>,
        timer: &mut impl Service<SyncStart, SyncComplete>,
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

        let timer_sender = timer.connect_client(box_builder.get_sender().sender_clone());
        let downloader = ClientMessageBox::new(downloader);
        let uploader = ClientMessageBox::new(uploader);

        fs_watcher.connect_sink(
            config.ops_dir.as_std_path().to_path_buf(),
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
            timer_sender,
            uploader,
            downloader,
            bridge_monitor_builder,
            message_handlers,
        })
    }

    pub async fn init(config: &C8yMapperConfig) -> Result<(), FileError> {
        // Create c8y operations directory
        create_directory_with_defaults(config.ops_dir.as_std_path()).await?;
        // Create directory for device custom fragments
        create_directory_with_defaults(config.config_dir.join("device")).await?;
        // Create directory for persistent entity store
        create_directory_with_defaults(config.state_dir.as_std_path()).await?;
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
        let mqtt_publisher = LoggingSender::new("C8yMapper => Mqtt".into(), self.mqtt_publisher);
        let timer_sender = LoggingSender::new("C8yMapper => Timer".into(), self.timer_sender);

        let converter = CumulocityConverter::new(
            self.config,
            mqtt_publisher.clone(),
            self.http_proxy,
            self.uploader,
            self.downloader,
        )
        .map_err(|err| RuntimeError::ActorError(Box::new(err)))?;

        let message_box = self.box_builder.build();
        let bridge_monitor_box = self.bridge_monitor_builder.build();

        Ok(C8yMapperActor::new(
            converter,
            message_box,
            mqtt_publisher,
            timer_sender,
            bridge_monitor_box,
            self.message_handlers,
        ))
    }
}
