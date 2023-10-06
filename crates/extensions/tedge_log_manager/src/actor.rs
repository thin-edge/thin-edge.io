use async_trait::async_trait;
use http::status::StatusCode;
use log::debug;
use log::error;
use log::info;
use log_manager::LogPluginConfig;
use serde_json::json;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::ClientMessageBox;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::NoMessage;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::Jsonify;
use tedge_file_system_ext::FsWatchEvent;
use tedge_http_ext::HttpError;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpRequestBuilder;
use tedge_http_ext::HttpResponseExt;
use tedge_http_ext::HttpResult;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;

use super::error::LogManagementError;
use super::LogManagerConfig;
use super::DEFAULT_PLUGIN_CONFIG_FILE_NAME;
use tedge_api::messages::CommandStatus;
use tedge_api::messages::LogUploadCmdPayload;

fan_in_message_type!(LogInput[MqttMessage, FsWatchEvent] : Debug);
fan_in_message_type!(LogOutput[MqttMessage]: Debug);

pub struct LogManagerActor {
    config: LogManagerConfig,
    plugin_config: LogPluginConfig,
    mqtt_publisher: LoggingSender<MqttMessage>,
    messages: SimpleMessageBox<LogInput, NoMessage>,
    http_proxy: ClientMessageBox<HttpRequest, HttpResult>,
}

#[async_trait]
impl Actor for LogManagerActor {
    fn name(&self) -> &str {
        "LogManager"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        self.reload_supported_log_types().await?;

        while let Some(event) = self.messages.recv().await {
            match event {
                LogInput::MqttMessage(message) => {
                    self.process_mqtt_message(message).await?;
                }
                LogInput::FsWatchEvent(event) => {
                    self.process_file_watch_events(event).await?;
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
        mqtt_publisher: LoggingSender<MqttMessage>,
        messages: SimpleMessageBox<LogInput, NoMessage>,
        http_proxy: ClientMessageBox<HttpRequest, HttpResult>,
    ) -> Self {
        Self {
            config,
            plugin_config,
            mqtt_publisher,
            messages,
            http_proxy,
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
                    CommandStatus::Successful | CommandStatus::Failed { .. } => {}
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
        match self.execute_logfile_request_operation(&request).await {
            Ok(()) => {
                request.successful();
                self.publish_command_status(topic, &request).await?;
                info!("Log request processed for log type: {}", request.log_type);
                Ok(())
            }
            Err(error) => {
                let error_message = format!("Handling of operation failed with {}", error);
                request.failed(&error_message);
                self.publish_command_status(topic, &request).await?;
                error!("{}", error_message);
                Ok(())
            }
        }
    }

    /// executes the log file request
    ///
    /// - sends request executing (mqtt)
    /// - uploads log content (http)
    /// - sends request successful (mqtt)
    async fn execute_logfile_request_operation(
        &mut self,
        request: &LogUploadCmdPayload,
    ) -> Result<(), LogManagementError> {
        let log_content = log_manager::new_read_logs(
            &self.plugin_config.files,
            &request.log_type,
            request.date_from,
            request.lines.to_owned(),
            &request.search_text,
        )?;

        self.send_log_file_http(log_content, request.tedge_url.clone())
            .await?;

        Ok(())
    }

    async fn send_log_file_http(
        &mut self,
        log_content: String,
        url: String,
    ) -> Result<(), LogManagementError> {
        let req_builder = HttpRequestBuilder::put(&url)
            .header("Content-Type", "text/plain")
            .body(log_content);

        let request = req_builder.build()?;

        let http_result = match self.http_proxy.await_response(request).await? {
            Ok(response) => match response.status() {
                StatusCode::OK | StatusCode::CREATED => Ok(response),
                code => Err(HttpError::HttpStatusError(code)),
            },
            Err(err) => Err(err),
        };

        let _ = http_result.error_for_status()?;
        info!("Logfile uploaded to: {}", url);
        Ok(())
    }

    async fn process_file_watch_events(&mut self, event: FsWatchEvent) -> Result<(), ChannelError> {
        let path = match event {
            FsWatchEvent::Modified(path) => path,
            FsWatchEvent::FileDeleted(path) => path,
            FsWatchEvent::FileCreated(path) => path,
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
        self.plugin_config = LogPluginConfig::new(self.config.plugin_config_path.as_path());
        self.publish_supported_log_types().await
    }

    /// updates the log types
    async fn publish_supported_log_types(&mut self) -> Result<(), ChannelError> {
        let mut config_types = self.plugin_config.get_all_file_types();
        config_types.sort();
        let payload = json!({ "types": config_types }).to_string();
        let msg = MqttMessage::new(&self.config.logtype_reload_topic, payload).with_retain();
        self.mqtt_publisher.send(msg).await
    }

    async fn publish_command_status(
        &mut self,
        topic: &Topic,
        request: &LogUploadCmdPayload,
    ) -> Result<(), ChannelError> {
        let message = request_into_message(topic, request);
        self.mqtt_publisher.send(message).await
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
