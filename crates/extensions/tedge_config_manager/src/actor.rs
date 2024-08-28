use anyhow::Context;
use async_trait::async_trait;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use log::error;
use log::info;
use serde_json::json;
use std::io::ErrorKind;
use std::sync::Arc;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::ClientMessageBox;
use tedge_actors::LoggingReceiver;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_api::commands::CommandPayload;
use tedge_api::commands::CommandStatus;
use tedge_api::commands::ConfigSnapshotCmdPayload;
use tedge_api::commands::ConfigUpdateCmdPayload;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicError;
use tedge_api::Jsonify;
use tedge_downloader_ext::DownloadRequest;
use tedge_downloader_ext::DownloadResult;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::Topic;
use tedge_uploader_ext::UploadRequest;
use tedge_uploader_ext::UploadResult;
use tedge_utils::atomic::MaybePermissions;
use tedge_write::CopyOptions;

use crate::TedgeWriteStatus;

use super::config::PluginConfig;
use super::error::ConfigManagementError;
use super::ConfigManagerConfig;
use super::DEFAULT_PLUGIN_CONFIG_FILE_NAME;

type MqttTopic = String;

pub type ConfigDownloadRequest = (MqttTopic, DownloadRequest);
pub type ConfigDownloadResult = (MqttTopic, DownloadResult);

pub type ConfigUploadRequest = (MqttTopic, UploadRequest);
pub type ConfigUploadResult = (MqttTopic, UploadResult);

fan_in_message_type!(ConfigInput[ConfigOperation, FsWatchEvent] : Debug);

pub struct ConfigManagerActor {
    config: ConfigManagerConfig,
    plugin_config: PluginConfig,
    input_receiver: LoggingReceiver<ConfigInput>,
    output_sender: LoggingSender<ConfigOperationData>,
    downloader: ClientMessageBox<ConfigDownloadRequest, ConfigDownloadResult>,
    uploader: ClientMessageBox<ConfigUploadRequest, ConfigUploadResult>,
}

#[async_trait]
impl Actor for ConfigManagerActor {
    fn name(&self) -> &str {
        "ConfigManager"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let mut worker = ConfigManagerWorker {
            config: Arc::from(self.config),
            plugin_config: self.plugin_config,
            output_sender: self.output_sender,
            downloader: self.downloader,
            uploader: self.uploader,
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
        plugin_config: PluginConfig,
        input_receiver: LoggingReceiver<ConfigInput>,
        output_sender: LoggingSender<ConfigOperationData>,
        downloader: ClientMessageBox<ConfigDownloadRequest, ConfigDownloadResult>,
        uploader: ClientMessageBox<ConfigUploadRequest, ConfigUploadResult>,
    ) -> Self {
        ConfigManagerActor {
            config,
            plugin_config,
            input_receiver,
            output_sender,
            downloader,
            uploader,
        }
    }
}

#[derive(Clone)]
struct ConfigManagerWorker {
    config: Arc<ConfigManagerConfig>,
    plugin_config: PluginConfig,
    output_sender: LoggingSender<ConfigOperationData>,
    downloader: ClientMessageBox<ConfigDownloadRequest, ConfigDownloadResult>,
    uploader: ClientMessageBox<ConfigUploadRequest, ConfigUploadResult>,
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
        let file_entry = self
            .plugin_config
            .get_file_entry_from_type(&request.config_type)?;

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

        let upload_request = UploadRequest::new(tedge_url, Utf8Path::new(&file_entry.path));

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
            "http://{}/tedge/file-transfer/{}/{}/{}-{}",
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
    ) -> Result<Utf8PathBuf, ConfigManagementError> {
        let file_entry = self
            .plugin_config
            .get_file_entry_from_type(&request.config_type)?;

        // because we might not have permissions to write to destination, save in tmpdir and then
        // move to destination later
        let temp_path = &self.config.tmp_path.join(&file_entry.config_type);

        let Some(tedge_url) = &request.tedge_url else {
            return Err(anyhow::anyhow!("tedge_url not present in config update payload").into());
        };

        let download_request = DownloadRequest::new(tedge_url, temp_path.as_std_path())
            .with_permission(file_entry.file_permissions.to_owned());

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

        let from = Utf8Path::from_path(download_response.file_path.as_path()).unwrap();
        let deployed_to_path = self
            .deploy_config_file(from, &request.config_type)
            .context("failed to deploy configuration file")?;

        // TODO: source temporary file should be cleaned up automatically
        let _ = std::fs::remove_file(from);

        Ok(deployed_to_path)
    }

    /// Deploys the new version of the configuration file and returns the path under which it was
    /// deployed.
    ///
    /// Ensures that the configuration file under `dest` is overwritten atomically by a new version
    /// currently stored in a temporary directory.
    ///
    /// If the configuration file doesn't already exist, a new file with target permissions is
    /// created. If the configuration file already exists, its content is overwritten, but owner and
    /// mode remains unchanged.
    ///
    /// If `use_tedge_write` is enabled, a `tedge-write` process is spawned when privilege elevation
    /// is required.
    fn deploy_config_file(
        &self,
        from: &Utf8Path,
        config_type: &str,
    ) -> Result<Utf8PathBuf, ConfigManagementError> {
        let file_entry = self.plugin_config.get_file_entry_from_type(config_type)?;

        let to = Utf8PathBuf::from(&file_entry.path);

        let permissions = MaybePermissions {
            uid: file_entry
                .file_permissions
                .user
                .as_ref()
                .map(|u| {
                    uzers::get_user_by_name(&u).with_context(|| format!("no such user: '{u}'"))
                })
                .transpose()?
                .map(|u| u.uid()),

            gid: file_entry
                .file_permissions
                .group
                .as_ref()
                .map(|g| {
                    uzers::get_group_by_name(&g).with_context(|| format!("no such group: '{g}'"))
                })
                .transpose()?
                .map(|g| g.gid()),

            mode: file_entry.file_permissions.mode,
        };

        let src = std::fs::File::open(from)
            .with_context(|| format!("failed to open source temporary file '{from}'"))?;

        let Err(err) = tedge_utils::atomic::write_file_atomic_set_permissions_if_doesnt_exist(
            src,
            &to,
            &permissions,
        )
        .with_context(|| format!("failed to deploy config file from '{from}' to '{to}'")) else {
            return Ok(to);
        };

        if let Some(io_error) = err.downcast_ref::<std::io::Error>() {
            if io_error.kind() != ErrorKind::PermissionDenied {
                return Err(err.into());
            }
        }

        match self.config.use_tedge_write.clone() {
            TedgeWriteStatus::Disabled => {
                return Err(err.into());
            }

            TedgeWriteStatus::Enabled { sudo } => {
                let mode = file_entry.file_permissions.mode;
                let user = file_entry.file_permissions.user.as_deref();
                let group = file_entry.file_permissions.group.as_deref();

                let options = CopyOptions {
                    from,
                    to: to.as_path(),
                    sudo,
                    mode,
                    user,
                    group,
                };

                options.copy()?;
            }
        }

        Ok(to)
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
                self.reload_supported_config_types().await?;
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

    async fn reload_supported_config_types(&mut self) -> Result<(), ChannelError> {
        self.plugin_config = PluginConfig::new(self.config.plugin_config_path.as_path());
        self.publish_supported_config_types().await
    }

    /// updates the config types
    async fn publish_supported_config_types(&mut self) -> Result<(), ChannelError> {
        let mut config_types = self.plugin_config.get_all_file_types();
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

    async fn publish_command_status(
        &mut self,
        operation: ConfigOperation,
    ) -> Result<(), ChannelError> {
        let state = ConfigOperationData::State(operation);
        self.output_sender.send(state).await
    }
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
