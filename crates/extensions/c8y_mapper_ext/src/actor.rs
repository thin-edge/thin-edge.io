use super::config::C8yMapperConfig;
use super::converter::CumulocityConverter;
use super::dynamic_discovery::process_inotify_events;
use crate::service_monitor::is_c8y_bridge_established;
use async_trait::async_trait;
use c8y_auth_proxy::url::ProxyUrlGenerator;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
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
use tedge_downloader_ext::DownloadRequest;
use tedge_downloader_ext::DownloadResult;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;
use tedge_uploader_ext::UploadRequest;
use tedge_uploader_ext::UploadResult;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::FileError;

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
    ) -> Self {
        Self {
            converter,
            messages,
            mqtt_publisher,
            timer_sender,
            bridge_status_messages,
        }
    }

    async fn process_mqtt_message(&mut self, message: MqttMessage) -> Result<(), RuntimeError> {
        let converted_messages = self.converter.convert(&message).await;

        for converted_message in converted_messages.into_iter() {
            self.mqtt_publisher.send(converted_message).await?;
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
                            self.mqtt_publisher
                                .send(
                                    self.converter
                                        .process_operation_update_message(discovered_ops),
                                )
                                .await?;
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
    auth_proxy: ProxyUrlGenerator,
    bridge_monitor_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
}

impl C8yMapperBuilder {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        config: C8yMapperConfig,
        mqtt: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
        http: &mut impl Service<C8YRestRequest, C8YRestResult>,
        timer: &mut impl Service<SyncStart, SyncComplete>,
        uploader: &mut impl Service<IdUploadRequest, IdUploadResult>,
        downloader: &mut impl Service<IdDownloadRequest, IdDownloadResult>,
        fs_watcher: &mut impl MessageSource<FsWatchEvent, PathBuf>,
        service_monitor: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
    ) -> Result<Self, FileError> {
        Self::init(&config)?;

        let box_builder: SimpleMessageBoxBuilder<C8yMapperInput, C8yMapperOutput> =
            SimpleMessageBoxBuilder::new("CumulocityMapper", 16);

        let mqtt_publisher = mqtt.get_sender();
        mqtt.connect_sink(config.topics.clone(), &box_builder.get_sender());
        let http_proxy = C8YHttpProxy::new(http);
        let timer_sender = timer.connect_client(box_builder.get_sender().sender_clone());

        let downloader = ClientMessageBox::new(downloader);
        let uploader = ClientMessageBox::new(uploader);

        fs_watcher.connect_sink(
            config.ops_dir.as_std_path().to_path_buf(),
            &box_builder.get_sender(),
        );
        let auth_proxy = ProxyUrlGenerator::new(
            config.auth_proxy_addr.clone(),
            config.auth_proxy_port,
            config.auth_proxy_protocol,
        );

        let bridge_monitor_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
            SimpleMessageBoxBuilder::new("ServiceMonitor", 1);

        service_monitor.connect_sink(
            config.bridge_health_topic.clone().into(),
            &bridge_monitor_builder,
        );

        Ok(Self {
            config,
            box_builder,
            mqtt_publisher,
            http_proxy,
            timer_sender,
            uploader,
            downloader,
            auth_proxy,
            bridge_monitor_builder,
        })
    }

    fn init(config: &C8yMapperConfig) -> Result<(), FileError> {
        // Create c8y operations directory
        create_directory_with_defaults(config.ops_dir.as_std_path())?;
        // Create directory for device custom fragments
        create_directory_with_defaults(config.config_dir.join("device"))?;
        // Create directory for persistent entity store
        create_directory_with_defaults(config.state_dir.as_std_path())?;
        Ok(())
    }
}

impl RuntimeRequestSink for C8yMapperBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
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
            self.auth_proxy,
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
        ))
    }
}
