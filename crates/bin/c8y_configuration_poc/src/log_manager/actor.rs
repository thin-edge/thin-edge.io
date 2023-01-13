use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;

use crate::c8y_http_proxy::handle::C8YHttpProxy;
use crate::file_system_ext::FsWatchEvent;
use async_trait::async_trait;
use c8y_api::smartrest::message::get_smartrest_device_id;
use c8y_api::smartrest::smartrest_deserializer::SmartRestLogRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRestSerializer;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToExecuting;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToFailed;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToSuccessful;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_api::OffsetDateTime;
use easy_reader::EasyReader;
use glob::glob;
use log::error;
use log::info;
use mqtt_channel::Message;
use mqtt_channel::StreamExt;
use mqtt_channel::TopicFilter;
use tedge_actors::fan_in_message_type;
use tedge_actors::mpsc;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;
use tedge_api::health::get_health_status_message;
use tedge_api::health::health_check_topics;
use tedge_mqtt_ext::MqttMessage;
use tedge_utils::paths::PathsError;

use super::error::LogRetrievalError;
use super::LogManagerConfig;
use super::LogPluginConfig;

fan_in_message_type!(LogInput[MqttMessage, FsWatchEvent] : Debug);
fan_in_message_type!(LogOutput[MqttMessage]: Debug);

pub struct LogManagerActor {
    config: LogManagerConfig,
    c8y_request_topics: TopicFilter,
    health_check_topics: TopicFilter,
    mqtt_publisher: DynSender<MqttMessage>,
    http_proxy: C8YHttpProxy,
}

impl LogManagerActor {
    pub fn new(
        config: LogManagerConfig,
        mqtt_publisher: DynSender<MqttMessage>,
        http_proxy: C8YHttpProxy,
    ) -> Self {
        let c8y_request_topics: TopicFilter = C8yTopic::SmartRestRequest.into();
        let health_check_topics = health_check_topics("c8y-log-plugin");

        Self {
            config,
            c8y_request_topics,
            health_check_topics,
            mqtt_publisher,
            http_proxy,
        }
    }

    pub async fn process_mqtt_message(
        &mut self,
        message: MqttMessage,
    ) -> Result<(), anyhow::Error> {
        if self.health_check_topics.accept(&message) {
            self.send_health_status().await?;
        } else if let Ok(payload) = message.payload_str() {
            for smartrest_message in payload.split('\n') {
                let result = match smartrest_message.split(',').next().unwrap_or_default() {
                    "522" => {
                        info!("Log request received: {payload}");
                        match get_smartrest_device_id(payload) {
                            Some(device_id) if device_id == self.config.device_id => {
                                // retrieve smartrest object from payload
                                let maybe_smartrest_obj =
                                    SmartRestLogRequest::from_smartrest(smartrest_message);
                                if let Ok(smartrest_obj) = maybe_smartrest_obj {
                                    self.handle_logfile_request_operation(&smartrest_obj).await
                                } else {
                                    error!("Incorrect SmartREST payload: {}", smartrest_message);
                                    Ok(())
                                }
                            }
                            // Ignore operation messages created for child devices
                            _ => Ok(()),
                        }
                    }
                    _ => {
                        // Ignore operation messages not meant for this plugin
                        Ok(())
                    }
                };

                if let Err(err) = result {
                    let error_message = format!(
                        "Handling of operation: '{}' failed with {}",
                        smartrest_message, err
                    );
                    error!("{}", error_message);
                }
            }
        }
        Ok(())
    }

    fn read_log_content(
        logfile: &Path,
        mut line_counter: usize,
        max_lines: usize,
        filter_text: &Option<String>,
    ) -> Result<(usize, String), LogRetrievalError> {
        if line_counter >= max_lines {
            Err(LogRetrievalError::MaxLines)
        } else {
            let mut file_content_as_vec = VecDeque::new();
            let file = std::fs::File::open(&logfile)?;
            let file_name = format!(
                "filename: {}\n",
                logfile.file_name().unwrap().to_str().unwrap() // never fails because we check file exists
            );
            let reader = EasyReader::new(file);
            match reader {
                Ok(mut reader) => {
                    reader.eof();
                    while line_counter < max_lines {
                        if let Some(haystack) = reader.prev_line()? {
                            if let Some(needle) = &filter_text {
                                if haystack.contains(needle) {
                                    file_content_as_vec.push_front(format!("{}\n", haystack));
                                    line_counter += 1;
                                }
                            } else {
                                file_content_as_vec.push_front(format!("{}\n", haystack));
                                line_counter += 1;
                            }
                        } else {
                            // there are no more lines.prev_line()
                            break;
                        }
                    }

                    file_content_as_vec.push_front(file_name);

                    let file_content = file_content_as_vec
                        .iter()
                        .map(|x| x.to_string())
                        .collect::<String>();
                    Ok((line_counter, file_content))
                }
                Err(_err) => Ok((line_counter, String::new())),
            }
        }
    }

    /// read any log file coming from `smartrest_obj.log_type`
    pub fn new_read_logs(
        &mut self,
        smartrest_obj: &SmartRestLogRequest,
    ) -> Result<String, anyhow::Error> {
        let mut output = String::new();
        // first filter logs on type
        let mut logfiles_to_read = self.filter_logs_on_type(smartrest_obj)?;
        logfiles_to_read = Self::filter_logs_path_on_metadata(smartrest_obj, logfiles_to_read)?;

        let mut line_counter = 0usize;
        for logfile in logfiles_to_read {
            match Self::read_log_content(
                logfile.as_path(),
                line_counter,
                smartrest_obj.lines,
                &smartrest_obj.needle,
            ) {
                Ok((lines, file_content)) => {
                    line_counter = lines;
                    output.push_str(&file_content);
                }
                Err(_error @ LogRetrievalError::MaxLines) => {
                    break;
                }
                Err(error) => {
                    return Err(error.into());
                }
            };
        }

        Ok(output)
    }

    fn filter_logs_on_type(
        &mut self,
        smartrest_obj: &SmartRestLogRequest,
    ) -> Result<Vec<PathBuf>, LogRetrievalError> {
        let mut files_to_send = Vec::new();
        for files in &self.config.plugin_config.files {
            let maybe_file_path = files.path.as_str(); // because it can be a glob pattern
            let file_type = files.config_type.as_str();

            if !file_type.eq(&smartrest_obj.log_type) {
                continue;
            } else {
                for entry in glob(maybe_file_path)? {
                    let file_path = entry?;
                    files_to_send.push(file_path)
                }
            }
        }
        if files_to_send.is_empty() {
            Err(LogRetrievalError::NoLogsAvailableForType {
                log_type: smartrest_obj.log_type.to_string(),
            })
        } else {
            Ok(files_to_send)
        }
    }

    /// filter a vector of pathbufs according to `smartrest_obj.date_from` and `smartrest_obj.date_to`
    fn filter_logs_path_on_metadata(
        smartrest_obj: &SmartRestLogRequest,
        mut logs_path_vec: Vec<PathBuf>,
    ) -> Result<Vec<PathBuf>, LogRetrievalError> {
        let mut out = vec![];

        logs_path_vec.sort_by_key(|pathbuf| {
            if let Ok(metadata) = std::fs::metadata(&pathbuf) {
                if let Ok(file_modified_time) = metadata.modified() {
                    return OffsetDateTime::from(file_modified_time);
                }
            };
            // if the file metadata can not be read, we set the file's metadata
            // to UNIX_EPOCH (Jan 1st 1970)
            OffsetDateTime::UNIX_EPOCH
        });
        logs_path_vec.reverse(); // to get most recent

        for file_pathbuf in logs_path_vec {
            let metadata = std::fs::metadata(&file_pathbuf)?;
            let datetime_modified = OffsetDateTime::from(metadata.modified()?);
            if datetime_modified >= smartrest_obj.date_from {
                out.push(file_pathbuf);
            }
        }

        if out.is_empty() {
            Err(LogRetrievalError::NoLogsAvailableForType {
                log_type: smartrest_obj.log_type.to_string(),
            })
        } else {
            Ok(out)
        }
    }

    /// executes the log file request
    ///
    /// - sends request executing (mqtt)
    /// - uploads log content (http)
    /// - sends request successful (mqtt)
    async fn execute_logfile_request_operation(
        &mut self,
        smartrest_request: &SmartRestLogRequest,
    ) -> Result<(), anyhow::Error> {
        let executing = LogfileRequest::executing()?;
        self.mqtt_publisher.send(executing).await?;

        let log_content = self.new_read_logs(smartrest_request)?;

        let upload_event_url = self
            .http_proxy
            .upload_log_binary(&smartrest_request.log_type, &log_content, None)
            .await?;

        let successful = LogfileRequest::successful(Some(upload_event_url))?;
        self.mqtt_publisher.send(successful).await?;

        info!("Log request processed.");
        Ok(())
    }
    pub async fn handle_logfile_request_operation(
        &mut self,
        smartrest_request: &SmartRestLogRequest,
    ) -> Result<(), anyhow::Error> {
        match self
            .execute_logfile_request_operation(smartrest_request)
            .await
        {
            Ok(()) => Ok(()),
            Err(error) => {
                let error_message = format!("Handling of operation failed with {}", error);
                let failed_msg = LogfileRequest::failed(error_message)?;
                self.mqtt_publisher.send(failed_msg).await?;
                Err(error)
            }
        }
    }

    pub async fn process_file_watch_events(
        &mut self,
        event: FsWatchEvent,
    ) -> Result<(), anyhow::Error> {
        let path = match event {
            FsWatchEvent::Modified(path) => path,
            FsWatchEvent::FileDeleted(path) => path,
            FsWatchEvent::FileCreated(path) => path,
            FsWatchEvent::DirectoryDeleted(_) => return Ok(()),
            FsWatchEvent::DirectoryCreated(_) => return Ok(()),
        };

        if path
            .file_name()
            .ok_or_else(|| PathsError::ParentDirNotFound {
                path: path.as_os_str().into(),
            })?
            .eq("c8y-log-plugin.toml")
        {}

        Ok(())
    }

    pub async fn reload_supported_log_types(&mut self) -> Result<(), anyhow::Error> {
        let plugin_config = LogPluginConfig::new(self.config.plugin_config_path.as_path());
        self.publish_supported_log_types(&plugin_config).await
    }

    /// updates the log types on Cumulocity
    /// sends 118,typeA,typeB,... on mqtt
    pub async fn publish_supported_log_types(
        &mut self,
        plugin_config: &LogPluginConfig,
    ) -> Result<(), anyhow::Error> {
        let msg = plugin_config.to_supported_config_types_message()?;
        Ok(self.mqtt_publisher.send(msg).await?)
    }

    async fn get_pending_operations_from_cloud(&mut self) -> Result<(), anyhow::Error> {
        // Get pending operations
        let msg = Message::new(&C8yTopic::SmartRestResponse.to_topic()?, "500");
        self.mqtt_publisher.send(msg).await?;
        Ok(())
    }

    async fn send_health_status(&mut self) -> Result<(), anyhow::Error> {
        let message = get_health_status_message("c8y-log-plugin").await;
        self.mqtt_publisher.send(message).await?;
        Ok(())
    }
}

#[async_trait]
impl Actor for LogManagerActor {
    type MessageBox = LogManagerMessageBox;

    fn name(&self) -> &str {
        "LogManager"
    }

    async fn run(mut self, mut messages: Self::MessageBox) -> Result<(), ChannelError> {
        self.reload_supported_log_types().await.unwrap();
        self.get_pending_operations_from_cloud().await.unwrap();

        while let Some(event) = messages.events.next().await {
            match event {
                LogInput::MqttMessage(message) => {
                    self.process_mqtt_message(message).await.unwrap();
                }
                LogInput::FsWatchEvent(event) => {
                    self.process_file_watch_events(event).await.unwrap();
                }
            }
        }
        Ok(())
    }
}

pub struct LogManagerMessageBox {
    pub events: mpsc::Receiver<LogInput>,
    pub mqtt_requests: DynSender<MqttMessage>,
}

impl LogManagerMessageBox {
    pub fn new(
        events: mpsc::Receiver<LogInput>,
        mqtt_con: DynSender<MqttMessage>,
    ) -> LogManagerMessageBox {
        LogManagerMessageBox {
            events,
            mqtt_requests: mqtt_con,
        }
    }
}

#[async_trait]
impl MessageBox for LogManagerMessageBox {
    type Input = LogInput;
    type Output = LogOutput;

    async fn recv(&mut self) -> Option<Self::Input> {
        tokio::select! {
            Some(message) = self.events.next() => {
                match message {
                    LogInput::MqttMessage(message) => {
                        Some(LogInput::MqttMessage(message))
                    },
                    LogInput::FsWatchEvent(message) => {
                        Some(LogInput::FsWatchEvent(message))
                    }
                }
            },
            else => None,
        }
    }

    async fn send(&mut self, message: Self::Output) -> Result<(), ChannelError> {
        match message {
            LogOutput::MqttMessage(msg) => self.mqtt_requests.send(msg).await,
        }
    }

    fn turn_logging_on(&mut self, _on: bool) {
        todo!()
    }

    fn name(&self) -> &str {
        "C8Y-Log-Manager"
    }

    fn logging_is_on(&self) -> bool {
        // FIXME this mailbox recv and send method are not used making logging ineffective.
        false
    }
}

pub struct LogfileRequest {}

impl TryIntoOperationStatusMessage for LogfileRequest {
    /// returns a c8y message specifying to set log status to executing.
    ///
    /// example message: '501,c8y_LogfileRequest'
    fn status_executing() -> Result<
        c8y_api::smartrest::smartrest_serializer::SmartRest,
        c8y_api::smartrest::error::SmartRestSerializerError,
    > {
        SmartRestSetOperationToExecuting::new(CumulocitySupportedOperations::C8yLogFileRequest)
            .to_smartrest()
    }

    fn status_successful(
        parameter: Option<String>,
    ) -> Result<
        c8y_api::smartrest::smartrest_serializer::SmartRest,
        c8y_api::smartrest::error::SmartRestSerializerError,
    > {
        SmartRestSetOperationToSuccessful::new(CumulocitySupportedOperations::C8yLogFileRequest)
            .with_response_parameter(&parameter.unwrap())
            .to_smartrest()
    }

    fn status_failed(
        failure_reason: String,
    ) -> Result<
        c8y_api::smartrest::smartrest_serializer::SmartRest,
        c8y_api::smartrest::error::SmartRestSerializerError,
    > {
        SmartRestSetOperationToFailed::new(
            CumulocitySupportedOperations::C8yLogFileRequest,
            failure_reason,
        )
        .to_smartrest()
    }
}
