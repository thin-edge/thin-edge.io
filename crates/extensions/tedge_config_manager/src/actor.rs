use anyhow::Context;
use async_trait::async_trait;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use log::debug;
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
use tedge_api::commands::CmdMetaSyncSignal;
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
use tedge_write::CreateDirsOptions;
use time::OffsetDateTime;

use crate::plugin::ExternalPlugin;
use crate::plugin_manager::ExternalPlugins;
use crate::FileEntry;
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

fan_in_message_type!(ConfigInput[ConfigOperation, CmdMetaSyncSignal, FsWatchEvent] : Debug);

pub struct ConfigManagerActor {
    config: ConfigManagerConfig,
    plugin_config: PluginConfig,
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
            plugin_config: self.plugin_config,
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
        external_plugins: ExternalPlugins,
    ) -> Self {
        ConfigManagerActor {
            config,
            plugin_config,
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
    plugin_config: PluginConfig,
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
        let config_path = if let Some(plugin) = self.get_plugin(&request.config_type)? {
            let (config_type, plugin_name) = split_cloud_config_type(&request.config_type)
                .expect("get_plugin returned Some, so config_type must have a plugin");
            let target_file = self.config.tmp_path.join(format!(
                "{}_{}_{}.conf",
                config_type,
                plugin_name,
                OffsetDateTime::now_utc().unix_timestamp()
            ));

            info!(
                target: "config plugins",
                "Retrieving config type: {} to file: {}", config_type, target_file
            );

            plugin.get(config_type, &target_file).await?;

            target_file.to_path_buf()
        } else {
            // No plugin specified; fall back to built-in file-based config handling
            let file_entry = self
                .plugin_config
                .get_file_entry_from_type(&request.config_type)?;
            Utf8PathBuf::from(&file_entry.path)
        };

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

        let config_path = if let Some(plugin) = self.get_plugin(&request.config_type)? {
            let (config_type, _) = split_cloud_config_type(&request.config_type)
                .expect("get_plugin returned Some, so config_type must have a plugin");
            info!(
                target: "config plugins",
                "Setting config type: {} from file: {}", config_type, from_path
            );

            plugin.set(config_type, from_path).await?;

            None
        } else {
            // No plugin specified; fall back to legacy file-based config handling
            let file_entry = self
                .plugin_config
                .get_file_entry_from_type(&request.config_type)?;
            let to = Utf8PathBuf::from(&file_entry.path);

            if let Some(parent) = to.parent() {
                if !parent.exists() {
                    self.create_parent_dirs(parent, file_entry)?;
                }
            }

            let deployed_to_path = self
                .deploy_config_file(from_path, file_entry)
                .context("failed to deploy configuration file")?;
            Some(deployed_to_path)
        };

        Ok(config_path)
    }

    /// Creates the parent directories of the target file if they are missing,
    /// and applies the permissions and ownership that are specified.
    /// First, if `use_tedge_write` is enabled, it tries to use tedge-write to create the missing parent directories.
    /// If it's disabled or creation with elevated privileges fails, fall back to the current user.
    fn create_parent_dirs(&self, parent: &Utf8Path, file_entry: &FileEntry) -> anyhow::Result<()> {
        if let TedgeWriteStatus::Enabled { sudo } = self.config.use_tedge_write.clone() {
            debug!("Creating the missing parent directories with elevation at '{parent}'");
            let result = CreateDirsOptions {
                dir_path: parent,
                sudo,
                mode: file_entry.parent_permissions.mode,
                user: file_entry.parent_permissions.user.as_deref(),
                group: file_entry.parent_permissions.group.as_deref(),
            }
            .create();

            match result {
                Ok(()) => return Ok(()),
                Err(err) => {
                    info!("Failed to create the missing parent directories with elevation at '{parent}' with error: {err}. \
            Falling back to the current user to create the directories.");
                }
            }
        }

        debug!("Creating the missing parent directories without elevation at '{parent}'");
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent directories. Path: '{parent}'"))?;

        file_entry.parent_permissions.clone().apply_sync(parent.as_std_path())
            .with_context(|| format!("failed to change permissions or mode of the parent directory. Path: '{parent}'"))?;

        Ok(())
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
        file_entry: &FileEntry,
    ) -> anyhow::Result<Utf8PathBuf> {
        let to = Utf8PathBuf::from(&file_entry.path);
        let permissions = MaybePermissions::try_from(&file_entry.file_permissions)?;

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
                return Err(err);
            }
        }

        match self.config.use_tedge_write.clone() {
            TedgeWriteStatus::Disabled => {
                return Err(err);
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
        self.plugin_config = PluginConfig::new(self.config.plugin_config_path.as_path());
        self.external_plugins.load().await?;

        self.publish_supported_config_types().await?;
        Ok(())
    }

    /// updates the config types
    async fn publish_supported_config_types(&mut self) -> Result<(), ChannelError> {
        let mut config_types = self.plugin_config.get_all_file_types();

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
                            config_types.push(build_cloud_config_type(&conf_type, &plugin_type));
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

    fn get_plugin(
        &self,
        config_type: &str,
    ) -> Result<Option<&ExternalPlugin>, ConfigManagementError> {
        if let Some((_, plugin_name)) = split_cloud_config_type(config_type) {
            if let Some(plugin) = self.external_plugins.by_plugin_type(plugin_name) {
                Ok(Some(plugin))
            } else {
                Err(ConfigManagementError::PluginError {
                    plugin_name: plugin_name.to_string(),
                    reason: "Plugin not found".to_string(),
                })
            }
        } else {
            Ok(None)
        }
    }

    async fn publish_command_status(
        &mut self,
        operation: ConfigOperation,
    ) -> Result<(), ChannelError> {
        let state = ConfigOperationData::State(operation);
        self.output_sender.send(state).await
    }
}

fn split_cloud_config_type(config_type: &str) -> Option<(&str, &str)> {
    config_type.split_once("::")
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
