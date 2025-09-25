use super::error::LogManagementError;
use super::LogManagerConfig;
use super::DEFAULT_PLUGIN_CONFIG_FILE_NAME;
use crate::plugin::Plugin;
use crate::plugin_manager::ExternalPlugins;
use crate::plugin_manager::Plugins;
use async_trait::async_trait;
use log::debug;
use log::error;
use log::info;
use log::warn;
use std::collections::HashMap;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::commands::CommandStatus;
use tedge_api::commands::LogUploadCmd;
use tedge_api::commands::LogUploadCmdMetadata;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandData;
use tedge_api::workflow::GenericCommandMetadata;
use tedge_api::workflow::GenericCommandState;
use tedge_api::Jsonify;
use tedge_file_system_ext::FsWatchEvent;
use tedge_uploader_ext::UploadRequest;
use tedge_uploader_ext::UploadResult;
use time::OffsetDateTime;

type MqttTopic = String;

pub type LogUploadRequest = (MqttTopic, UploadRequest);
pub type LogUploadResult = (MqttTopic, UploadResult);

fan_in_message_type!(LogInput[LogUploadCmd, FsWatchEvent, LogUploadResult] : Debug);
fan_in_message_type!(LogOutput[LogUploadCmd, LogUploadCmdMetadata] : Debug);

impl LogOutput {
    pub fn into_generic_command(self) -> Option<GenericCommandData> {
        match self {
            LogOutput::LogUploadCmd(cmd) => Some(GenericCommandState::from(cmd).into()),
            LogOutput::LogUploadCmdMetadata(metadata) => Some(
                GenericCommandMetadata {
                    operation: OperationType::LogUpload.to_string(),
                    payload: metadata.to_value(),
                }
                .into(),
            ),
        }
    }
}

pub struct LogManagerActor {
    config: LogManagerConfig,
    pending_operations: HashMap<String, LogUploadCmd>,
    messages: SimpleMessageBox<LogInput, LogOutput>,
    upload_sender: DynSender<LogUploadRequest>,
    external_plugins: ExternalPlugins,
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
                LogInput::LogUploadCmd(request) => {
                    self.process_logfile_request(request).await?;
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
        messages: SimpleMessageBox<LogInput, LogOutput>,
        upload_sender: DynSender<LogUploadRequest>,
        external_plugins: ExternalPlugins,
    ) -> Self {
        Self {
            config,
            pending_operations: HashMap::new(),
            messages,
            upload_sender,
            external_plugins,
        }
    }

    pub async fn process_logfile_request(
        &mut self,
        request: LogUploadCmd,
    ) -> Result<(), ChannelError> {
        match request.status() {
            CommandStatus::Init | CommandStatus::Scheduled => {
                info!("Log request received: {request:?}");
                self.start_executing_logfile_request(request).await?;
            }
            CommandStatus::Executing => {
                debug!("Executing log request: {request:?}");
                self.handle_logfile_request_operation(request).await?;
            }
            CommandStatus::Unknown | CommandStatus::Successful | CommandStatus::Failed { .. } => {}
        }

        Ok(())
    }

    pub async fn start_executing_logfile_request(
        &mut self,
        mut request: LogUploadCmd,
    ) -> Result<(), ChannelError> {
        request.executing();
        self.publish_command_status(request).await
    }

    pub async fn handle_logfile_request_operation(
        &mut self,
        mut request: LogUploadCmd,
    ) -> Result<(), ChannelError> {
        if let Err(error) = self.generate_and_upload_logfile(&request).await {
            let error_message = format!("Failed to initiate log file upload: {error}");
            request.failed(&error_message);
            self.publish_command_status(request).await?;
            error!("{}", error_message);
            return Ok(());
        }

        let topic = request.topic(&self.config.mqtt_schema).as_ref().to_string();
        self.pending_operations.insert(topic, request);

        Ok(())
    }

    /// Generates the required logfile and starts its upload via the uploader actor.
    async fn generate_and_upload_logfile(
        &mut self,
        request: &LogUploadCmd,
    ) -> Result<(), LogManagementError> {
        let topic = request.topic(&self.config.mqtt_schema).as_ref().to_string();
        let request_payload = &request.payload;

        let (log_type, plugin_name) = request_payload
            .log_type
            .split_once("::")
            .unwrap_or((&request_payload.log_type, "file"));

        let log_path = if let Some(plugin) = self.external_plugins.by_plugin_type(plugin_name) {
            let output_log_path = self.config.tmp_dir.join(format!(
                "{}_{}_{}.log",
                log_type,
                plugin_name,
                OffsetDateTime::now_utc().unix_timestamp()
            ));

            plugin
                .get(
                    log_type,
                    output_log_path.as_path(),
                    Some(request_payload.date_from),
                    Some(request_payload.date_to),
                    request_payload.search_text.as_deref(),
                    Some(request_payload.lines),
                )
                .await?;

            output_log_path.to_path_buf()
        } else {
            return Err(LogManagementError::PluginError {
                plugin_name: plugin_name.to_string(),
                reason: "Plugin not found".to_string(),
            });
        };

        let upload_request = UploadRequest::new(&request_payload.tedge_url, log_path.as_path());

        info!(
            "Awaiting upload of log type: {} to url: {}",
            request_payload.log_type, request_payload.tedge_url
        );

        self.upload_sender.send((topic, upload_request)).await?;

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

        match result {
            Ok(response) => {
                request.successful();

                info!(
                    "Log request processed for log type: {}.",
                    request.payload.log_type
                );

                if let Err(err) = std::fs::remove_file(&response.file_path) {
                    warn!(
                        "Failed to remove temporary file {}: {}",
                        response.file_path, err
                    )
                }

                self.publish_command_status(request).await?;
            }
            Err(err) => {
                let error_message = format!("Failed to upload log to file-transfer service: {err}");
                request.failed(&error_message);
                error!("{}", error_message);
                self.publish_command_status(request).await?;
            }
        }

        Ok(())
    }

    async fn process_file_watch_events(&mut self, event: FsWatchEvent) -> Result<(), RuntimeError> {
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

        if path.parent() == Some(&self.config.plugins_dir)
            || path
                .file_name()
                .is_some_and(|name| name.eq(DEFAULT_PLUGIN_CONFIG_FILE_NAME))
        {
            self.reload_supported_log_types().await?;
        }

        Ok(())
    }

    async fn reload_supported_log_types(&mut self) -> Result<(), RuntimeError> {
        info!("Reloading supported log types");

        // Note: The log manager now only handles external plugins.
        // The file-based plugin configuration is handled by the standalone plugin.
        self.external_plugins.load().await?;
        self.publish_supported_log_types().await?;

        Ok(())
    }

    /// updates the log types
    async fn publish_supported_log_types(&mut self) -> Result<(), ChannelError> {
        let mut types = Vec::new();

        // Add external plugin log types with ::plugin_name suffix
        for plugin_type in self.external_plugins.get_all_plugin_types() {
            warn!("Listing log types using plugin: {}", plugin_type);
            if let Some(plugin) = self.external_plugins.by_plugin_type(&plugin_type) {
                match plugin.list(None).await {
                    Ok(log_types) => {
                        warn!("Plugin {} supports log types: {:?}", plugin_type, log_types);
                        for log_type in log_types {
                            if plugin_type == "file" {
                                // For the file plugin, add log types without suffix (default behavior)
                                types.push(log_type);
                            } else {
                                // For other plugins, add with suffix
                                types.push(format!("{}::{}", log_type, plugin_type));
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to get log types from plugin {}: {}", plugin_type, e);
                    }
                }
            }
        }

        types.sort();

        let metadata = LogUploadCmdMetadata { types };
        self.messages
            .send(LogOutput::LogUploadCmdMetadata(metadata))
            .await
    }

    async fn publish_command_status(&mut self, request: LogUploadCmd) -> Result<(), ChannelError> {
        self.messages.send(LogOutput::LogUploadCmd(request)).await
    }
}
