use anyhow::Context;
use async_trait::async_trait;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use log::error;
use log::info;
use serde_json::json;
use serde_json::Value;
use std::sync::Arc;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::ClientMessageBox;
use tedge_actors::LoggingReceiver;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RequestEnvelope;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_api::commands::CmdMetaSyncSignal;
use tedge_api::commands::CommandPayload;
use tedge_api::commands::CommandStatus;
use tedge_api::commands::ConfigSnapshotCmdPayload;
use tedge_api::commands::ConfigUpdateCmdPayload;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicError;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::OperationStepRequest;
use tedge_api::workflow::OperationStepResponse;
use tedge_api::CommandLog;
use tedge_api::Jsonify;
use tedge_downloader_ext::DownloadRequest;
use tedge_downloader_ext::DownloadResult;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::Topic;
use tedge_uploader_ext::UploadRequest;
use tedge_uploader_ext::UploadResult;
use time::OffsetDateTime;

use crate::plugin::ExternalPlugin;
use crate::plugin_manager::parse_config_type;
use crate::plugin_manager::ExternalPlugins;

use super::error::ConfigManagementError;
use super::ConfigManagerConfig;
use super::DEFAULT_PLUGIN_CONFIG_FILE_NAME;

type MqttTopic = String;

pub type ConfigDownloadRequest = (MqttTopic, DownloadRequest);
pub type ConfigDownloadResult = (MqttTopic, DownloadResult);

pub type ConfigUploadRequest = (MqttTopic, UploadRequest);
pub type ConfigUploadResult = (MqttTopic, UploadResult);

pub type OperationStepRequestEnvelope =
    RequestEnvelope<OperationStepRequest, OperationStepResponse>;

fan_in_message_type!(ConfigInput[ConfigOperation, CmdMetaSyncSignal, FsWatchEvent, OperationStepRequestEnvelope] : Debug);

pub struct ConfigManagerActor {
    config: ConfigManagerConfig,
    input_receiver: LoggingReceiver<ConfigInput>,
    output_sender: LoggingSender<ConfigOperationData>,
    downloader: ClientMessageBox<ConfigDownloadRequest, ConfigDownloadResult>,
    uploader: ClientMessageBox<ConfigUploadRequest, ConfigUploadResult>,
    external_plugins: ExternalPlugins,
}

#[async_trait]
impl Actor for ConfigManagerActor {
    fn name(&self) -> &str {
        "ConfigManager"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let mut worker = ConfigManagerWorker {
            config: Arc::from(self.config),
            output_sender: self.output_sender,
            downloader: self.downloader,
            uploader: self.uploader,
            external_plugins: self.external_plugins,
        };

        worker.reload_supported_config_types().await?;

        while let Some(event) = self.input_receiver.recv().await {
            let result = match event {
                ConfigInput::ConfigOperation(request) => {
                    let mut worker = worker.clone();
                    tokio::spawn(async move { worker.process_operation_request(request).await });
                    Ok(())
                }
                ConfigInput::FsWatchEvent(event) => worker.process_file_watch_events(event).await,
                ConfigInput::CmdMetaSyncSignal(_) => worker.reload_supported_config_types().await,
                ConfigInput::OperationStepRequestEnvelope(request) => {
                    let mut worker = worker.clone();
                    tokio::spawn(async move {
                        worker.process_operation_step_request(request).await;
                    });
                    Ok(())
                }
            };

            if let Err(err) = result {
                error!("Error processing event: {err:?}");
            }
        }

        Ok(())
    }
}

impl ConfigManagerActor {
    pub fn new(
        config: ConfigManagerConfig,
        input_receiver: LoggingReceiver<ConfigInput>,
        output_sender: LoggingSender<ConfigOperationData>,
        downloader: ClientMessageBox<ConfigDownloadRequest, ConfigDownloadResult>,
        uploader: ClientMessageBox<ConfigUploadRequest, ConfigUploadResult>,
        external_plugins: ExternalPlugins,
    ) -> Self {
        ConfigManagerActor {
            config,
            input_receiver,
            output_sender,
            downloader,
            uploader,
            external_plugins,
        }
    }
}

#[derive(Clone)]
struct ConfigManagerWorker {
    config: Arc<ConfigManagerConfig>,
    output_sender: LoggingSender<ConfigOperationData>,
    downloader: ClientMessageBox<ConfigDownloadRequest, ConfigDownloadResult>,
    uploader: ClientMessageBox<ConfigUploadRequest, ConfigUploadResult>,
    external_plugins: ExternalPlugins,
}

impl ConfigManagerWorker {
    async fn process_operation_request(
        &mut self,
        request: ConfigOperation,
    ) -> Result<(), ChannelError> {
        match request {
            ConfigOperation::Snapshot(topic, request) => match request.status {
                CommandStatus::Init | CommandStatus::Scheduled => {
                    info!("Config Snapshot received: {request:?}");
                    self.start_executing_config_request(ConfigOperation::Snapshot(topic, request))
                        .await?;
                }
                CommandStatus::Executing => {
                    info!("Executing Config Snapshot request: {request:?}");
                    self.handle_config_snapshot_request(topic, request).await?;
                }
                CommandStatus::Unknown
                | CommandStatus::Successful
                | CommandStatus::Failed { .. } => {}
            },
            ConfigOperation::Update(topic, request) => match request.status {
                CommandStatus::Init | CommandStatus::Scheduled => {
                    info!("Config Update received: {request:?}");
                    self.start_executing_config_request(ConfigOperation::Update(topic, request))
                        .await?;
                }
                CommandStatus::Executing => {
                    info!("Executing Config Update request: {request:?}");
                    self.handle_config_update_request(topic, request).await?;
                }
                CommandStatus::Unknown
                | CommandStatus::Successful
                | CommandStatus::Failed { .. } => {}
            },
        }
        Ok(())
    }

    async fn start_executing_config_request(
        &mut self,
        mut operation: ConfigOperation,
    ) -> Result<(), ChannelError> {
        match operation {
            ConfigOperation::Snapshot(ref topic, ref mut request) => {
                match &request.tedge_url {
                    Some(_) => request.executing(None),
                    None => {
                        match self
                            .create_tedge_url_for_config_operation(topic, &request.config_type)
                        {
                            Ok(tedge_url) => request.executing(Some(tedge_url)),
                            Err(err) => request.failed(format!("Failed to create tedgeUrl: {err}")),
                        };
                    }
                };
            }
            ConfigOperation::Update(_, ref mut request, ..) => {
                // FIXME: using the remote url for the tedge url bypasses the operation file cache
                if request.tedge_url.is_none() {
                    request.tedge_url = Some(request.remote_url.clone());
                };
                request.executing();
            }
        }
        self.publish_command_status(operation).await
    }

    async fn handle_config_snapshot_request(
        &mut self,
        topic: Topic,
        mut request: ConfigSnapshotCmdPayload,
    ) -> Result<(), ChannelError> {
        match self
            .execute_config_snapshot_request(&topic, &mut request)
            .await
        {
            Ok(file_path) => {
                request.successful(file_path.as_str());
                info!(
                    "Config Snapshot request processed for config type: {}.",
                    request.config_type
                );
                self.publish_command_status(ConfigOperation::Snapshot(topic, request))
                    .await?;
            }
            Err(error) => {
                request.failed(error.to_string());
                error!("config-manager failed to process config snapshot: {error}");
                self.publish_command_status(ConfigOperation::Snapshot(topic, request))
                    .await?;
            }
        }
        Ok(())
    }

    async fn execute_config_snapshot_request(
        &mut self,
        topic: &Topic,
        request: &mut ConfigSnapshotCmdPayload,
    ) -> Result<Utf8PathBuf, ConfigManagementError> {
        let (config_type, plugin_name) = parse_config_type(&request.config_type);
        let plugin = self.get_plugin(plugin_name)?;

        let config_path = self.config.tmp_path.join(format!(
            "{}_{}_{}.conf",
            config_type,
            plugin_name,
            OffsetDateTime::now_utc().unix_timestamp()
        ));

        // Extract cmd_id from topic for CommandLog
        let cmd_id = self.extract_command_id(topic)?;

        // Create CommandLog from log_path if present
        let mut command_log = request.log_path.clone().map(|path| {
            CommandLog::from_log_path(path, OperationType::ConfigSnapshot.to_string(), cmd_id)
        });

        info!(
            target: "config plugins",
            "Retrieving config type: {} to file: {}", config_type, config_path
        );
        plugin
            .get(config_type, &config_path, command_log.as_mut())
            .await?;

        let tedge_url = match &request.tedge_url {
            Some(tedge_url) => tedge_url,
            None => {
                request.executing(Some(
                    self.create_tedge_url_for_config_operation(topic, &request.config_type)?,
                ));
                // Safe to unwrap because we've just created the url
                request.tedge_url.as_ref().unwrap()
            }
        };

        let upload_request = UploadRequest::new(tedge_url, config_path.as_path());

        info!(
            "Awaiting upload of config type: {} to url: {}",
            request.config_type, tedge_url
        );

        let (_, upload_result) = self
            .uploader
            .await_response((topic.name.clone(), upload_request))
            .await?;

        let upload_response =
            upload_result.context("config-manager failed uploading configuration snapshot")?;

        Ok(upload_response.file_path)
    }

    fn create_tedge_url_for_config_operation(
        &self,
        topic: &Topic,
        config_type: &str,
    ) -> Result<String, EntityTopicError> {
        let (device_name, operation_type, cmd_id) =
            match self.config.mqtt_schema.entity_channel_of(topic) {
                Ok((entity, Channel::Command { operation, cmd_id })) => {
                    match entity.default_device_name() {
                        Some(device_name) => (device_name.to_owned(), operation, cmd_id),
                        None => {
                            return Err(EntityTopicError::TopicId(
                                tedge_api::mqtt_topics::TopicIdError::InvalidMqttTopic,
                            ))
                        }
                    }
                }

                _ => {
                    return Err(EntityTopicError::Channel(
                        tedge_api::mqtt_topics::ChannelError::InvalidCategory(topic.name.clone()),
                    ));
                }
            };

        Ok(format!(
            "http://{}/te/v1/files/{}/{}/{}-{}",
            &self.config.tedge_http_host,
            device_name,
            operation_type,
            config_type.replace('/', ":"),
            cmd_id
        ))
    }

    async fn handle_config_update_request(
        &mut self,
        topic: Topic,
        mut request: ConfigUpdateCmdPayload,
    ) -> Result<(), ChannelError> {
        match self.execute_config_update_request(&topic, &request).await {
            Ok(deployed_to_path) => {
                request.successful(deployed_to_path);
                info!(
                    "Config Update request processed for config type: {}.",
                    request.config_type
                );
                self.publish_command_status(ConfigOperation::Update(topic, request))
                    .await?;
            }
            Err(error) => {
                request.failed(error.to_string());
                error!("config-manager failed to process config update: {error}");
                self.publish_command_status(ConfigOperation::Update(topic, request))
                    .await?;
            }
        }
        Ok(())
    }

    async fn execute_config_update_request(
        &mut self,
        topic: &Topic,
        request: &ConfigUpdateCmdPayload,
    ) -> Result<Option<Utf8PathBuf>, ConfigManagementError> {
        // because we might not have permissions to write to destination, save in tmpdir and then
        // move to destination later
        let temp_path = &self.config.tmp_path.join(&request.config_type);

        let Some(tedge_url) = &request.tedge_url else {
            return Err(anyhow::anyhow!("tedge_url not present in config update payload").into());
        };

        let download_request = DownloadRequest::new(tedge_url, temp_path.as_std_path());

        info!(
            "Awaiting download for config type: {} from url: {}",
            request.config_type, tedge_url
        );

        let (_, download_result) = self
            .downloader
            .await_response((topic.name.clone(), download_request))
            .await?;

        let download_response =
            download_result.context("config-manager failed downloading a file")?;

        let from = tempfile::TempPath::from_path(download_response.file_path);

        let from_path = Utf8Path::from_path(&from)
            .with_context(|| format!("path is not utf-8: '{}'", from.to_string_lossy()))?;

        let cmd_id = self.extract_command_id(topic)?;

        self.execute_config_set_step(
            topic,
            &request.config_type,
            from_path,
            request.log_path.clone(),
            &cmd_id,
        )
        .await?;

        Ok(None)
    }

    async fn process_operation_step_request(
        &mut self,
        mut req: RequestEnvelope<OperationStepRequest, OperationStepResponse>,
    ) {
        let topic = req.request.command_state.topic.clone();
        let command_step = req.request.command_step;
        let command = req.request.command_state;
        let result = match command_step.as_str() {
            "set" => self.process_config_set_request(command).await,
            _ => Err(ConfigManagementError::InvalidOperationStep(
                command_step.clone(),
            )),
        };

        let response = result
            .map_err(|err| format!("config_operation step: '{}' failed: {}", command_step, err));

        if let Err(error) = req.reply_to.send(response).await {
            error!(
                "Failed to send OperationStepResponse for command on topic: {} due to: {}",
                topic, error
            );
        }
    }

    async fn process_config_set_request(
        &mut self,
        command: GenericCommandState,
    ) -> Result<(), ConfigManagementError> {
        let topic = command.topic.clone();
        let cmd_id = self.extract_command_id(&topic)?;

        let log_path = command.get_log_path();

        let config_type = command
            .payload
            .get("type")
            .and_then(|v: &Value| v.as_str())
            .map(|s: &str| s.to_string())
            .ok_or_else(|| ConfigManagementError::MissingKey("type".to_string()))?;
        let from_path = command
            .payload
            .get("setFrom")
            .and_then(|v: &Value| v.as_str())
            .map(|s: &str| Utf8PathBuf::from(s))
            .ok_or_else(|| ConfigManagementError::MissingKey("setFrom".to_string()))?;

        self.execute_config_set_step(&topic, &config_type, &from_path, log_path, &cmd_id)
            .await
    }

    async fn execute_config_set_step(
        &mut self,
        _topic: &Topic,
        config_type: &str,
        from_path: &Utf8Path,
        log_path: Option<Utf8PathBuf>,
        cmd_id: &str,
    ) -> Result<(), ConfigManagementError> {
        if !from_path.exists() {
            return Err(ConfigManagementError::FileNotFound(from_path.to_string()));
        }

        let (config_type, plugin_type) = parse_config_type(config_type);
        let plugin = self
            .external_plugins
            .by_plugin_type(plugin_type)
            .ok_or_else(|| ConfigManagementError::PluginNotFound(plugin_type.to_string()))?;

        let mut command_log = log_path.clone().map(|path| {
            CommandLog::from_log_path(
                path,
                OperationType::ConfigUpdate.to_string(),
                cmd_id.to_string(),
            )
        });

        plugin
            .set(config_type, from_path, command_log.as_mut())
            .await?;

        Ok(())
    }

    fn extract_command_id(&self, topic: &Topic) -> Result<String, ConfigManagementError> {
        match self.config.mqtt_schema.entity_channel_of(topic) {
            Ok((_, Channel::Command { cmd_id, .. })) => Ok(cmd_id),
            _ => Err(ConfigManagementError::InvalidCommandTopic(
                topic.name.clone(),
            )),
        }
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

        let plugin_changed = self
            .config
            .plugin_dirs
            .iter()
            .any(|plugin_dir| path.starts_with(plugin_dir));
        if plugin_changed
            || path
                .file_name()
                .is_some_and(|name| name.eq(DEFAULT_PLUGIN_CONFIG_FILE_NAME))
        {
            self.reload_supported_config_types().await?;
        }

        Ok(())
    }

    async fn reload_supported_config_types(&mut self) -> Result<(), RuntimeError> {
        info!(target: "config plugins", "Reloading supported config types");
        self.external_plugins.load();

        self.publish_supported_config_types().await?;
        Ok(())
    }

    /// updates the config types
    async fn publish_supported_config_types(&mut self) -> Result<(), ChannelError> {
        let mut config_types = Vec::new();

        // Add external plugin config types with ::plugin_name suffix
        for plugin_type in self.external_plugins.get_all_plugin_types() {
            if let Some(plugin) = self.external_plugins.by_plugin_type(&plugin_type) {
                match plugin.list(None).await {
                    Ok(conf_types) => {
                        info!(
                            target: "config plugins",
                            "Plugin {} supports config types: {:?}", plugin_type, conf_types
                        );

                        for conf_type in conf_types {
                            if plugin_type == "file" {
                                // For the file plugin, add config types without suffix (default behavior)
                                config_types.push(conf_type);
                            } else {
                                // For other plugins, add with suffix
                                config_types
                                    .push(build_cloud_config_type(&conf_type, &plugin_type));
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            target: "config plugins",
                            "Failed to get config types from plugin {}: {}", plugin_type, e
                        );
                    }
                }
            }
        }

        config_types.sort();

        for topic in self.config.config_reload_topics.iter() {
            let metadata = ConfigOperationData::Metadata {
                topic: topic.clone(),
                types: config_types.clone(),
            };
            self.output_sender.send(metadata).await?;
        }
        Ok(())
    }

    fn get_plugin(&self, plugin_name: &str) -> Result<&ExternalPlugin, ConfigManagementError> {
        self.external_plugins
            .by_plugin_type(plugin_name)
            .ok_or_else(|| ConfigManagementError::PluginError {
                plugin_name: plugin_name.to_string(),
                reason: "Plugin not found".to_string(),
            })
    }

    async fn publish_command_status(
        &mut self,
        operation: ConfigOperation,
    ) -> Result<(), ChannelError> {
        let state = ConfigOperationData::State(operation);
        self.output_sender.send(state).await
    }
}

fn build_cloud_config_type(config_type: &str, plugin_name: &str) -> String {
    format!("{}::{}", config_type, plugin_name)
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConfigOperation {
    Snapshot(Topic, ConfigSnapshotCmdPayload),
    Update(Topic, ConfigUpdateCmdPayload),
}

impl ConfigOperation {
    pub(crate) fn request_from_message(
        config: &ConfigManagerConfig,
        message: &MqttMessage,
    ) -> Result<Option<Self>, ConfigManagementError> {
        if message.payload_bytes().is_empty() {
            Ok(None)
        } else if config.config_snapshot_topic.accept(message) {
            Ok(Some(ConfigOperation::Snapshot(
                message.topic.clone(),
                ConfigSnapshotCmdPayload::from_json(message.payload_str()?)?,
            )))
        } else if config.config_update_topic.accept(message) {
            Ok(Some(ConfigOperation::Update(
                message.topic.clone(),
                ConfigUpdateCmdPayload::from_json(message.payload_str()?)?,
            )))
        } else {
            Err(ConfigManagementError::InvalidTopicError)
        }
    }

    fn request_into_message(&self) -> MqttMessage {
        match self {
            ConfigOperation::Snapshot(topic, request) => MqttMessage::new(topic, request.to_json())
                .with_retain()
                .with_qos(QoS::AtLeastOnce),
            ConfigOperation::Update(topic, request) => MqttMessage::new(topic, request.to_json())
                .with_retain()
                .with_qos(QoS::AtLeastOnce),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConfigOperationData {
    State(ConfigOperation),
    Metadata { topic: Topic, types: Vec<String> },
}

impl From<ConfigOperationData> for MqttMessage {
    fn from(value: ConfigOperationData) -> Self {
        match value {
            ConfigOperationData::State(cmd) => cmd.request_into_message(),
            ConfigOperationData::Metadata { topic, types } => {
                let payload = json!({ "types": types }).to_string();
                MqttMessage::new(&topic, payload)
                    .with_retain()
                    .with_qos(QoS::AtLeastOnce)
            }
        }
    }
}
