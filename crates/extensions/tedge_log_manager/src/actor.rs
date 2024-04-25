use std::collections::HashMap;

use async_trait::async_trait;
use camino::Utf8Path;
use log::debug;
use log::error;
use log::info;
use log::warn;
use log_manager::LogPluginConfig;
use serde_json::json;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::commands::CommandPayload;
use tedge_api::commands::CommandStatus;
use tedge_api::commands::LogUploadCmdPayload;
use tedge_api::Jsonify;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_uploader_ext::UploadRequest;
use tedge_uploader_ext::UploadResult;

use super::error::LogManagementError;
use super::LogManagerConfig;
use super::DEFAULT_PLUGIN_CONFIG_FILE_NAME;

type MqttTopic = String;

pub type LogUploadRequest = (MqttTopic, UploadRequest);
pub type LogUploadResult = (MqttTopic, UploadResult);

fan_in_message_type!(LogInput[MqttMessage, FsWatchEvent, LogUploadResult] : Debug);

pub struct LogManagerActor {
    config: LogManagerConfig,
    plugin_config: LogPluginConfig,
    pending_operations: HashMap<String, LogUploadCmdPayload>,
    messages: SimpleMessageBox<LogInput, MqttMessage>,
    upload_sender: DynSender<LogUploadRequest>,
}

#[async_trait]
impl Actor for LogManagerActor {
    fn name(&self) -> &str {
        "LogManager"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        self.reload_supported_log_types().await?;

        while let Some(event) = self.messages.recv().await {
            match event {
                LogInput::MqttMessage(message) => {
                    self.process_mqtt_message(message).await?;
                }
                LogInput::FsWatchEvent(event) => {
                    self.process_file_watch_events(event).await?;
                }
                LogInput::LogUploadResult((topic, result)) => {
                    self.process_uploaded_log(&topic, result).await?;
                }
            }
        }
        Ok(())
    }
}

impl LogManagerActor {
    pub fn new(
        config: LogManagerConfig,
        plugin_config: LogPluginConfig,
        messages: SimpleMessageBox<LogInput, MqttMessage>,
        upload_sender: DynSender<LogUploadRequest>,
    ) -> Self {
        Self {
            config,
            plugin_config,
            pending_operations: HashMap::new(),
            messages,
            upload_sender,
        }
    }

    pub async fn process_mqtt_message(&mut self, message: MqttMessage) -> Result<(), ChannelError> {
        if self.config.logfile_request_topic.accept(&message) {
            match request_from_message(&message) {
                Ok(Some(request)) => match request.status {
                    CommandStatus::Init => {
                        info!("Log request received: {request:?}");
                        self.start_executing_logfile_request(&message.topic, request)
                            .await?;
                    }
                    CommandStatus::Executing => {
                        debug!("Executing log request: {request:?}");
                        self.handle_logfile_request_operation(&message.topic, request)
                            .await?;
                    }
                    CommandStatus::Scheduled
                    | CommandStatus::Unknown
                    | CommandStatus::Successful
                    | CommandStatus::Failed { .. } => {}
                },
                Ok(None) => {}
                Err(err) => {
                    error!("Incorrect log request payload: {}", err);
                }
            }
        } else {
            error!(
                "Received unexpected message on topic: {}",
                message.topic.name
            );
        }

        Ok(())
    }

    pub async fn start_executing_logfile_request(
        &mut self,
        topic: &Topic,
        mut request: LogUploadCmdPayload,
    ) -> Result<(), ChannelError> {
        request.executing();
        self.publish_command_status(topic, &request).await
    }

    pub async fn handle_logfile_request_operation(
        &mut self,
        topic: &Topic,
        mut request: LogUploadCmdPayload,
    ) -> Result<(), ChannelError> {
        if let Err(error) = self.generate_and_upload_logfile(topic, &request).await {
            let error_message = format!("Failed to initiate log file upload: {error}");
            request.failed(&error_message);
            self.publish_command_status(topic, &request).await?;
            error!("{}", error_message);
            return Ok(());
        }

        self.pending_operations.insert(topic.name.clone(), request);

        Ok(())
    }

    /// Generates the required logfile and starts its upload via the uploader actor.
    async fn generate_and_upload_logfile(
        &mut self,
        topic: &Topic,
        request: &LogUploadCmdPayload,
    ) -> Result<(), LogManagementError> {
        let log_path = log_manager::new_read_logs(
            &self.plugin_config.files,
            &request.log_type,
            request.date_from,
            request.lines.to_owned(),
            &request.search_text,
            &self.config.tmp_dir,
        )?;

        let upload_request = UploadRequest::new(
            &request.tedge_url,
            Utf8Path::from_path(log_path.as_path()).unwrap(),
        );

        info!(
            "Awaiting upload of log type: {} to url: {}",
            request.log_type, request.tedge_url
        );

        self.upload_sender
            .send((topic.name.clone(), upload_request))
            .await?;

        Ok(())
    }

    async fn process_uploaded_log(
        &mut self,
        topic: &str,
        result: UploadResult,
    ) -> Result<(), LogManagementError> {
        let Some(mut request) = self.pending_operations.remove(topic) else {
            warn!("Ignoring unexpected log_upload result: {topic}");
            return Ok(());
        };

        let topic = Topic::new_unchecked(topic);
        match result {
            Ok(response) => {
                request.successful();

                info!("Log request processed for log type: {}.", request.log_type);

                if let Err(err) = std::fs::remove_file(&response.file_path) {
                    warn!(
                        "Failed to remove temporary file {}: {}",
                        response.file_path, err
                    )
                }

                self.publish_command_status(&topic, &request).await?;
            }
            Err(err) => {
                let error_message = format!("Failed to upload log to file-transfer service: {err}");
                request.failed(&error_message);
                error!("{}", error_message);
                self.publish_command_status(&topic, &request).await?;
            }
        }

        Ok(())
    }

    async fn process_file_watch_events(&mut self, event: FsWatchEvent) -> Result<(), ChannelError> {
        let path = match event {
            FsWatchEvent::Modified(path) => path,
            FsWatchEvent::FileDeleted(path) => path,
            // Creating new files and file moves and copies also emits `FsWatchEvent::Modified`
            // _most_ of the time, so we don't have to listen to `FileCreated`, if we did we'd have
            // duplicates.
            //
            // https://github.com/thin-edge/thin-edge.io/pull/2454#discussion_r1394358034
            FsWatchEvent::FileCreated(_) => return Ok(()),
            FsWatchEvent::DirectoryDeleted(_) => return Ok(()),
            FsWatchEvent::DirectoryCreated(_) => return Ok(()),
        };

        match path.file_name() {
            Some(path) if path.eq(DEFAULT_PLUGIN_CONFIG_FILE_NAME) => {
                self.reload_supported_log_types().await?;
                Ok(())
            }
            Some(_) => Ok(()),
            None => {
                error!(
                    "Path for {} does not exist",
                    DEFAULT_PLUGIN_CONFIG_FILE_NAME
                );
                Ok(())
            }
        }
    }

    async fn reload_supported_log_types(&mut self) -> Result<(), ChannelError> {
        info!("Reloading supported log types");

        self.plugin_config = LogPluginConfig::new(self.config.plugin_config_path.as_path());
        self.publish_supported_log_types().await
    }

    /// updates the log types
    async fn publish_supported_log_types(&mut self) -> Result<(), ChannelError> {
        let mut config_types = self.plugin_config.get_all_file_types();
        config_types.sort();
        let payload = json!({ "types": config_types }).to_string();
        let msg = MqttMessage::new(&self.config.logtype_reload_topic, payload).with_retain();
        self.messages.send(msg).await
    }

    async fn publish_command_status(
        &mut self,
        topic: &Topic,
        request: &LogUploadCmdPayload,
    ) -> Result<(), ChannelError> {
        let message = request_into_message(topic, request);
        self.messages.send(message).await
    }
}

fn request_from_message(
    message: &MqttMessage,
) -> Result<Option<LogUploadCmdPayload>, LogManagementError> {
    if message.payload_bytes().is_empty() {
        Ok(None)
    } else {
        Ok(Some(LogUploadCmdPayload::from_json(
            message.payload_str()?,
        )?))
    }
}

fn request_into_message(topic: &Topic, request: &LogUploadCmdPayload) -> MqttMessage {
    MqttMessage::new(topic, request.to_json()).with_retain()
}
