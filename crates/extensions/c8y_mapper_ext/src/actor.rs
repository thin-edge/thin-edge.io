use super::config::C8yMapperConfig;
use super::converter::CumulocityConverter;
use super::dynamic_discovery::process_inotify_events;
use crate::operations::FtsDownloadOperationType;
use async_trait::async_trait;
use c8y_api::smartrest::smartrest_serializer::fail_operation;
use c8y_api::smartrest::smartrest_serializer::succeed_static_operation;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRest;
use c8y_auth_proxy::url::ProxyUrlGenerator;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tedge_actors::adapt;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sender;
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::entity_store::EntityType;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_downloader_ext::DownloadRequest;
use tedge_downloader_ext::DownloadResult;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::Message;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::TopicFilter;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;
use tedge_uploader_ext::UploadRequest;
use tedge_uploader_ext::UploadResult;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::FileError;
use tracing::error;
use tracing::warn;

const SYNC_WINDOW: Duration = Duration::from_secs(3);

pub type SyncStart = SetTimeout<()>;
pub type SyncComplete = Timeout<()>;

pub(crate) type CmdId = String;
pub(crate) type IdUploadRequest = (CmdId, UploadRequest);
pub(crate) type IdUploadResult = (CmdId, UploadResult);
pub(crate) type IdDownloadResult = (CmdId, DownloadResult);
pub(crate) type IdDownloadRequest = (CmdId, DownloadRequest);

fan_in_message_type!(C8yMapperInput[MqttMessage, FsWatchEvent, SyncComplete, IdUploadResult, IdDownloadResult] : Debug);
type C8yMapperOutput = MqttMessage;

pub struct C8yMapperActor {
    converter: CumulocityConverter,
    messages: SimpleMessageBox<C8yMapperInput, C8yMapperOutput>,
    mqtt_publisher: LoggingSender<MqttMessage>,
    timer_sender: LoggingSender<SyncStart>,
}

#[async_trait]
impl Actor for C8yMapperActor {
    fn name(&self) -> &str {
        "CumulocityMapper"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
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
                C8yMapperInput::IdUploadResult((cmd_id, result)) => {
                    self.process_upload_result(cmd_id, result).await?;
                }
                C8yMapperInput::IdDownloadResult((cmd_id, result)) => {
                    self.process_download_result(cmd_id, result).await?;
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
    ) -> Self {
        Self {
            converter,
            messages,
            mqtt_publisher,
            timer_sender,
        }
    }

    async fn process_mqtt_message(&mut self, message: MqttMessage) -> Result<(), RuntimeError> {
        let converted_messages = self.converter.convert(&message).await;

        for converted_message in converted_messages.into_iter() {
            self.mqtt_publisher.send(converted_message).await?;
        }

        Ok(())
    }

    /// Registers the entity under a given MQTT topic.
    ///
    /// If a given entity was registered previously, the function will do
    /// nothing. Otherwise it will save registration data to memory, free to be
    /// queried by other components.
    // fn register_entity(&mut self, topic: String, payload: String) {
    //     self.entity_store.entry(&topic).or_insert(payload);
    // }

    async fn process_file_watch_event(
        &mut self,
        file_event: FsWatchEvent,
    ) -> Result<(), RuntimeError> {
        match file_event.clone() {
            FsWatchEvent::DirectoryCreated(path) => {
                if let Some(directory_name) = path.file_name() {
                    let dir_name = directory_name.to_string_lossy().to_string();
                    let child_external_id =
                        match CumulocityConverter::validate_external_id(&dir_name) {
                            Ok(name) => name,
                            Err(err) => {
                                error!(
                                    "Child device directory: {} ignored due to {}",
                                    &dir_name, err
                                );
                                return Ok(());
                            }
                        };

                    let child_name = self
                        .converter
                        .default_device_name_from_external_id(&child_external_id);
                    let child_topic_id = EntityTopicId::default_child_device(&child_name).unwrap();
                    let child_device_reg_msg = EntityRegistrationMessage {
                        topic_id: child_topic_id,
                        external_id: Some(child_external_id),
                        r#type: EntityType::ChildDevice,
                        parent: None,
                        other: json!({ "name": child_name })
                            .as_object()
                            .unwrap()
                            .to_owned(),
                    };
                    match self
                        .converter
                        .register_and_convert_entity(&child_device_reg_msg)
                    {
                        Ok(messages) => {
                            for message in messages {
                                self.mqtt_publisher.send(message).await?;
                            }
                        }
                        Err(err) => {
                            error!(
                                "Processing dynamic child device directory creation failed with {}",
                                err
                            );
                        }
                    }
                }
            }
            FsWatchEvent::FileCreated(path)
            | FsWatchEvent::FileDeleted(path)
            | FsWatchEvent::Modified(path)
            | FsWatchEvent::DirectoryDeleted(path) => {
                match process_inotify_events(&self.converter.ops_dir, &path, file_event) {
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

    async fn process_upload_result(
        &mut self,
        cmd_id: CmdId,
        upload_result: UploadResult,
    ) -> Result<(), RuntimeError> {
        match self.converter.pending_upload_operations.remove(&cmd_id) {
            None => error!("Received an upload result for the unknown command ID: {cmd_id}"),
            Some(queued_data) => {
                let payload = match queued_data.operation {
                    CumulocitySupportedOperations::C8yLogFileRequest
                    | CumulocitySupportedOperations::C8yUploadConfigFile => self
                        .get_smartrest_response_for_upload_result(
                            upload_result,
                            &queued_data.c8y_binary_url,
                            queued_data.operation,
                        ),
                    other_type => {
                        warn!("Received unsupported operation {other_type:?} for uploading a file");
                        return Ok(());
                    }
                };

                let c8y_notification = Message::new(&queued_data.smartrest_topic, payload);
                let clear_local_cmd = Message::new(&queued_data.clear_cmd_topic, "")
                    .with_retain()
                    .with_qos(QoS::AtLeastOnce);
                for converted_message in [c8y_notification, clear_local_cmd] {
                    self.mqtt_publisher.send(converted_message).await?
                }
            }
        };

        Ok(())
    }

    fn get_smartrest_response_for_upload_result(
        &self,
        upload_result: UploadResult,
        binary_url: &str,
        operation: CumulocitySupportedOperations,
    ) -> SmartRest {
        match upload_result {
            Ok(_) => succeed_static_operation(operation, Some(binary_url)),
            Err(err) => fail_operation(operation, &format!("Upload failed with {err}")),
        }
    }

    async fn process_download_result(
        &mut self,
        cmd_id: CmdId,
        result: DownloadResult,
    ) -> Result<(), RuntimeError> {
        // download not from c8y_proxy, check if it was from FTS
        let operation_result = if let Some(fts_download) = self
            .converter
            .pending_fts_download_operations
            .remove(&cmd_id)
        {
            let cmd_id = cmd_id.clone();
            match fts_download.download_type {
                FtsDownloadOperationType::ConfigDownload => {
                    self.converter
                        .handle_fts_config_download_result(cmd_id, result, fts_download)
                        .await
                }
                FtsDownloadOperationType::LogDownload => {
                    self.converter
                        .handle_fts_log_download_result(cmd_id, result, fts_download)
                        .await
                }
            }
        } else {
            error!("Received a download result for the unknown command ID: {cmd_id}");
            return Ok(());
        };

        match operation_result {
            Ok(converted_messages) => {
                for converted_message in converted_messages.into_iter() {
                    self.mqtt_publisher.send(converted_message).await?
                }
            }
            Err(err) => {
                error!("Error occurred while processing a download result. {err}")
            }
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
    upload_sender: DynSender<IdUploadRequest>,
    download_sender: DynSender<IdDownloadRequest>,
    auth_proxy: ProxyUrlGenerator,
}

impl C8yMapperBuilder {
    pub fn try_new(
        config: C8yMapperConfig,
        mqtt: &mut impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter>,
        http: &mut impl ServiceProvider<C8YRestRequest, C8YRestResult, NoConfig>,
        timer: &mut impl ServiceProvider<SyncStart, SyncComplete, NoConfig>,
        uploader: &mut impl ServiceProvider<IdUploadRequest, IdUploadResult, NoConfig>,
        downloader: &mut impl ServiceProvider<IdDownloadRequest, IdDownloadResult, NoConfig>,
        fs_watcher: &mut impl MessageSource<FsWatchEvent, PathBuf>,
    ) -> Result<Self, FileError> {
        Self::init(&config)?;

        let box_builder = SimpleMessageBoxBuilder::new("CumulocityMapper", 16);

        let mqtt_publisher =
            mqtt.connect_consumer(config.topics.clone(), adapt(&box_builder.get_sender()));
        let http_proxy = C8YHttpProxy::new("C8yMapper => C8YHttpProxy", http);
        let timer_sender = timer.connect_consumer(NoConfig, adapt(&box_builder.get_sender()));
        let upload_sender = uploader.connect_consumer(NoConfig, adapt(&box_builder.get_sender()));
        let download_sender =
            downloader.connect_consumer(NoConfig, adapt(&box_builder.get_sender()));
        fs_watcher.register_peer(config.ops_dir.clone(), adapt(&box_builder.get_sender()));
        let auth_proxy = ProxyUrlGenerator::new(
            config.auth_proxy_addr.clone(),
            config.auth_proxy_port,
            config.auth_proxy_protocol,
        );

        Ok(Self {
            config,
            box_builder,
            mqtt_publisher,
            http_proxy,
            timer_sender,
            upload_sender,
            download_sender,
            auth_proxy,
        })
    }

    fn init(config: &C8yMapperConfig) -> Result<(), FileError> {
        // Create c8y operations directory
        create_directory_with_defaults(config.ops_dir.clone())?;
        // Create directory for device custom fragments
        create_directory_with_defaults(config.config_dir.join("device"))?;
        // Create directory for persistent entity store
        create_directory_with_defaults(&config.state_dir)?;
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
        let uploader_sender =
            LoggingSender::new("C8yMapper => Uploader".into(), self.upload_sender);
        let downloader_sender =
            LoggingSender::new("C8yMapper => Downloader".into(), self.download_sender);

        let converter = CumulocityConverter::new(
            self.config,
            mqtt_publisher.clone(),
            self.http_proxy,
            self.auth_proxy,
            uploader_sender.clone(),
            downloader_sender.clone(),
        )
        .map_err(|err| RuntimeError::ActorError(Box::new(err)))?;

        let message_box = self.box_builder.build();

        Ok(C8yMapperActor::new(
            converter,
            message_box,
            mqtt_publisher,
            timer_sender,
        ))
    }
}
