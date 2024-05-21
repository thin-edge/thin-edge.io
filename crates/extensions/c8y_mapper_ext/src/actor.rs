use super::config::C8yMapperConfig;
use super::converter::CumulocityConverter;
use super::dynamic_discovery::process_inotify_events;
use crate::converter::UploadContext;
use crate::converter::UploadOperationLog;
use crate::operations::FtsDownloadOperationType;
use async_trait::async_trait;
use c8y_api::smartrest::smartrest_serializer::fail_operation;
use c8y_api::smartrest::smartrest_serializer::succeed_static_operation;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRest;
use c8y_api::utils::bridge::is_c8y_bridge_established;
use c8y_auth_proxy::url::ProxyUrlGenerator;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
use camino::Utf8Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
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
use tedge_api::main_device_health_topic;
use tedge_downloader_ext::DownloadRequest;
use tedge_downloader_ext::DownloadResult;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::TopicFilter;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;
use tedge_uploader_ext::UploadRequest;
use tedge_uploader_ext::UploadResult;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::FileError;
use tokio::sync::Mutex;
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

#[derive(Clone)]
struct MqttPublisher(Arc<Mutex<LoggingSender<MqttMessage>>>);

impl MqttPublisher {
    pub fn new(mqtt_publisher: LoggingSender<MqttMessage>) -> Self {
        Self(Arc::new(Mutex::new(mqtt_publisher)))
    }

    pub async fn send(&self, message: MqttMessage) -> Result<(), ChannelError> {
        self.0.lock().await.send(message).await
    }
}

pub struct C8yMapperActor {
    converter: CumulocityConverter,
    messages: SimpleMessageBox<C8yMapperInput, C8yMapperOutput>,
    mqtt_publisher: MqttPublisher,
    timer_sender: LoggingSender<SyncStart>,
    bridge_status_messages: SimpleMessageBox<MqttMessage, MqttMessage>,
    c8y_bridge_service_name: String,
}

pub struct C8yMapperWorker {
    // converter methods that handle download and upload state changes are mutable, so need to
    // synchronize them with a mutex
    converter: Mutex<CumulocityConverter>,
    mqtt_publisher: MqttPublisher,
    ops_dir: Arc<Utf8Path>,
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
                if is_c8y_bridge_established(&message, &self.c8y_bridge_service_name) {
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

        let mut messages = self.messages;

        let ops_dir = Arc::clone(&self.converter.config.ops_dir);
        let worker = Arc::new(C8yMapperWorker {
            converter: Mutex::new(self.converter),
            mqtt_publisher: self.mqtt_publisher,
            ops_dir,
        });

        while let Some(event) = messages.recv().await {
            let worker = Arc::clone(&worker);

            tokio::spawn(async move {
                match event {
                    C8yMapperInput::MqttMessage(message) => {
                        // request message
                        worker.process_mqtt_message(message).await?;
                    }
                    C8yMapperInput::FsWatchEvent(event) => {
                        // request message
                        worker.process_file_watch_event(event).await?;
                    }
                    C8yMapperInput::SyncComplete(_) => {
                        // request message
                        worker.process_sync_timeout().await?;
                    }
                    C8yMapperInput::IdUploadResult((cmd_id, result)) => {
                        // immediate message
                        worker.process_upload_result(cmd_id, result).await?;
                    }
                    C8yMapperInput::IdDownloadResult((cmd_id, result)) => {
                        // immediate message
                        worker.process_download_result(cmd_id, result).await?;
                    }
                }

                Ok::<(), RuntimeError>(())
            });
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
        c8y_bridge_service_name: String,
    ) -> Self {
        Self {
            converter,
            messages,
            mqtt_publisher: MqttPublisher::new(mqtt_publisher),
            timer_sender,
            bridge_status_messages,
            c8y_bridge_service_name,
        }
    }
}

impl C8yMapperWorker {
    async fn process_mqtt_message(&self, message: MqttMessage) -> Result<(), RuntimeError> {
        let converted_messages = self.converter.lock().await.convert(&message).await;

        for converted_message in converted_messages.into_iter() {
            self.mqtt_publisher.send(converted_message).await?;
        }

        Ok(())
    }

    async fn process_file_watch_event(&self, file_event: FsWatchEvent) -> Result<(), RuntimeError> {
        match file_event.clone() {
            FsWatchEvent::FileCreated(path)
            | FsWatchEvent::FileDeleted(path)
            | FsWatchEvent::Modified(path) => {
                // Process inotify events only for the main device at the root operations directory
                // directly under /etc/tedge/operations/c8y
                if path.parent() == Some(self.ops_dir.as_std_path()) {
                    match process_inotify_events(self.ops_dir.as_std_path(), &path, file_event) {
                        Ok(Some(discovered_ops)) => {
                            self.mqtt_publisher
                                .send(
                                    self.converter
                                        .lock()
                                        .await
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

    pub async fn process_sync_timeout(&self) -> Result<(), RuntimeError> {
        // Once the sync phase is complete, retrieve all sync messages from the converter and process them
        let sync_messages = self.converter.lock().await.sync_messages();
        for message in sync_messages {
            self.process_mqtt_message(message).await?;
        }

        Ok(())
    }

    async fn process_upload_result(
        &self,
        cmd_id: CmdId,
        upload_result: UploadResult,
    ) -> Result<(), RuntimeError> {
        let pending_upload = self
            .converter
            .lock()
            .await
            .pending_upload_operations
            .remove(&cmd_id);

        match pending_upload {
            Some(UploadContext::OperationData(queued_data)) => {
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

                let c8y_notification = MqttMessage::new(&queued_data.smartrest_topic, payload);
                let clear_local_cmd = MqttMessage::new(&queued_data.clear_cmd_topic, "")
                    .with_retain()
                    .with_qos(QoS::AtLeastOnce);

                let messages = self
                    .converter
                    .lock()
                    .await
                    .upload_operation_log(
                        &queued_data.topic_id,
                        &cmd_id,
                        &queued_data.operation.into(),
                        queued_data.command,
                        vec![c8y_notification, clear_local_cmd],
                    )
                    .await;
                for message in messages {
                    self.mqtt_publisher.send(message).await?
                }
            }
            Some(UploadContext::OperationLog(UploadOperationLog { final_messages })) => {
                for message in final_messages {
                    self.mqtt_publisher.send(message).await?
                }
                return Ok(());
            }
            None => error!("Received an upload result for the unknown command ID: {cmd_id}"),
        }

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
        &self,
        cmd_id: CmdId,
        result: DownloadResult,
    ) -> Result<(), RuntimeError> {
        // download not from c8y_proxy, check if it was from FTS
        let fts_download_operation = self
            .converter
            .lock()
            .await
            .pending_fts_download_operations
            .remove(&cmd_id);

        let operation_result = if let Some(fts_download) = fts_download_operation {
            let cmd_id = cmd_id.clone();
            match fts_download.download_type {
                FtsDownloadOperationType::ConfigDownload => {
                    // self.converter
                    //     .lock()
                    //     .await
                    //     .handle_fts_config_download_result(cmd_id, result, fts_download)
                    //     .await
                    Ok(vec![])
                }
                FtsDownloadOperationType::LogDownload => {
                    self.converter
                        .lock()
                        .await
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
        let upload_sender = uploader.connect_client(box_builder.get_sender().sender_clone());
        let download_sender = downloader.connect_client(box_builder.get_sender().sender_clone());
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
        let bridge_health_topic = main_device_health_topic(&config.bridge_service_name());
        service_monitor.connect_sink(
            bridge_health_topic.as_str().try_into().unwrap(),
            &bridge_monitor_builder,
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
        let uploader_sender =
            LoggingSender::new("C8yMapper => Uploader".into(), self.upload_sender);
        let downloader_sender =
            LoggingSender::new("C8yMapper => Downloader".into(), self.download_sender);
        let c8y_bridge_service_name = self.config.bridge_service_name();

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
        let bridge_monitor_box = self.bridge_monitor_builder.build();

        Ok(C8yMapperActor::new(
            converter,
            message_box,
            mqtt_publisher,
            timer_sender,
            bridge_monitor_box,
            c8y_bridge_service_name,
        ))
    }
}
