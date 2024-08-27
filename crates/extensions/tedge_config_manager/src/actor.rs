use anyhow::Context;
use async_trait::async_trait;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use log::error;
use log::info;
use serde_json::json;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::os::unix::fs::fchown;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
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
use tedge_write::CopyOptions;

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

fan_in_message_type!(ConfigInput[ConfigOperation, FsWatchEvent] : Debug);

pub struct ConfigManagerActor {
    config: ConfigManagerConfig,
    plugin_config: PluginConfig,
    pending_operations: HashMap<String, ConfigOperation>,
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
            pending_operations: self.pending_operations,
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
            pending_operations: HashMap::new(),
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
    pending_operations: HashMap<String, ConfigOperation>,
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
            Ok(upload_result) => {
                self.pending_operations.insert(
                    topic.name.clone(),
                    ConfigOperation::Snapshot(topic.clone(), request),
                );

                self.process_uploaded_config(&topic.name, upload_result)
                    .await?;
            }
            Err(error) => {
                let error_message = format!(
                    "Failed to initiate configuration snapshot upload to file-transfer service: {error}",
                );
                request.failed(&error_message);
                error!("{}", error_message);
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
    ) -> Result<UploadResult, ConfigManagementError> {
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

        Ok(upload_result)
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

    async fn process_uploaded_config(
        &mut self,
        topic: &str,
        result: UploadResult,
    ) -> Result<(), ChannelError> {
        if let Some(ConfigOperation::Snapshot(topic, mut request)) =
            self.pending_operations.remove(topic)
        {
            match result {
                Ok(response) => {
                    request.successful(response.file_path.as_str());
                    info!(
                        "Config Snapshot request processed for config type: {}.",
                        request.config_type
                    );
                    self.publish_command_status(ConfigOperation::Snapshot(topic, request))
                        .await?;
                }
                Err(err) => {
                    let error_message = format!(
                        "config-manager failed uploading configuration snapshot: {}",
                        err
                    );
                    request.failed(&error_message);
                    error!("{}", error_message);
                    self.publish_command_status(ConfigOperation::Snapshot(topic, request))
                        .await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_config_update_request(
        &mut self,
        topic: Topic,
        mut request: ConfigUpdateCmdPayload,
    ) -> Result<(), ChannelError> {
        match self.execute_config_update_request(&topic, &request).await {
            Ok(download_result) => {
                self.pending_operations.insert(
                    topic.name.clone(),
                    ConfigOperation::Update(topic.clone(), request),
                );

                self.process_downloaded_config(&topic.name, download_result)
                    .await?;
            }
            Err(error) => {
                let error_message = format!(
                    "config-manager failed to start downloading configuration: {}",
                    error
                );
                request.failed(&error_message);
                error!("{}", error_message);
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
    ) -> Result<DownloadResult, ConfigManagementError> {
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

        Ok(download_result)
    }

    async fn process_downloaded_config(
        &mut self,
        topic: &str,
        result: DownloadResult,
    ) -> Result<(), ChannelError> {
        let Some(ConfigOperation::Update(topic, mut request)) =
            self.pending_operations.remove(topic)
        else {
            return Ok(());
        };

        let response = match result {
            Ok(response) => response,
            Err(err) => {
                let err =
                    anyhow::Error::from(err).context("config-manager failed downloading a file");
                let error_message = format!("{err:#}");
                request.failed(&error_message);
                error!("{}", error_message);
                self.publish_command_status(ConfigOperation::Update(topic, request))
                    .await?;
                return Ok(());
            }
        };

        // new config was downloaded into tmpdir, we need to write it into destination using tedge-write
        let from = Utf8Path::from_path(response.file_path.as_path()).unwrap();

        let deployed_to_path = match self.deploy_config_file(from, &request.config_type) {
            Ok(path) => path,
            Err(err) => {
                let error_message =
                    format!("config-manager failed writing updated configuration file: {err}",);

                request.failed(&error_message);
                error!("{}", error_message);
                self.publish_command_status(ConfigOperation::Update(topic, request))
                    .await?;

                // TODO: source temporary file should be cleaned up automatically
                let _ = std::fs::remove_file(from);

                return Ok(());
            }
        };

        request.successful(deployed_to_path);
        info!(
            "Config Update request processed for config type: {}.",
            request.config_type
        );
        self.publish_command_status(ConfigOperation::Update(topic, request))
            .await?;

        // TODO: source temporary file should be cleaned up automatically
        let _ = std::fs::remove_file(from);

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
        config_type: &str,
    ) -> Result<Utf8PathBuf, ConfigManagementError> {
        let file_entry = self.plugin_config.get_file_entry_from_type(config_type)?;

        let to = Utf8PathBuf::from(&file_entry.path);

        let Err(err) = move_file_atomic_set_permissions_if_doesnt_exist(from, file_entry)
            .with_context(|| format!("failed to deploy config file from '{from}' to '{to}'"))
        else {
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

/// Writes a file atomically and optionally sets its permissions.
///
/// Setting permissions (owner and group) of a file is a privileged operation so it needs to be run
/// as root. If the any of the filesystem operations fail due to not having permissions, the
/// function will return an error.
///
/// If the file already exists, its content will be overwritten but its permissions will remain
/// unchanged.
///
/// For deployment of configuration files, we need to create a file atomically because certain
/// programs might watch configuration file for changes, so if it's not written atomically, then
/// file might be only partially written and a program trying to read it may crash.
///
/// Atomic write of a file consists of creating a temporary file in the same directory, filling it
/// with correct content and permissions, and only then renaming the temporary into the destination
/// filename. Because we're never actually writing into the file, we don't need to write permissions
/// for the destination file, even if it exists. Instead we need only write/execute permissions to
/// the directory file is located in unless the directory has a sticky bit set. Overwriting a file
/// will also change its uid/gid/mode, if writing process euid/egid is different from file's
/// uid/gid. To keep uid/gid the same, after the write we need to do `chown`, and to do it we need
/// sudo.
///
/// # Errors
///
/// - `src` doesn't exist
/// - user or group doesn't exist
/// - we have no write/execute permissions to the directory
fn move_file_atomic_set_permissions_if_doesnt_exist(
    src: &Utf8Path,
    file_entry: &FileEntry,
) -> anyhow::Result<()> {
    let dest = Utf8Path::new(file_entry.path.as_str());

    let target_permissions = config_file_target_permissions(file_entry, dest)
        .context("failed to compute target permissions of the file")?;

    let mut src_file = std::fs::File::open(src)
        .with_context(|| format!("failed to open temporary source file '{src}'"))?;

    // TODO: create tests to ensure writes we expect are atomic
    let mut tempfile = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o600))
        .tempfile_in(dest.parent().context("invalid path")?)
        .with_context(|| format!("could not create temporary file at '{dest}'"))?;

    std::io::copy(&mut src_file, &mut tempfile).context("failed to copy")?;

    tempfile
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(target_permissions.mode))
        .context("failed to set mode on the destination file")?;

    fchown(
        tempfile.as_file(),
        Some(target_permissions.uid),
        Some(target_permissions.gid),
    )
    .context("failed to change ownership of the destination file")?;

    tempfile.as_file().sync_all()?;

    tempfile
        .persist(dest)
        .context("failed to persist temporary file at destination")?;

    Ok(())
}

/// Computes target permissions for deployment of the config file.
///
/// - if file exists preserve current permissions
/// - if it doesn't exist apply permissions from `permissions` if they are defined
/// - set to root:root with default umask otherwise
///
/// # Errors
/// - if desired user/group doesn't exist on the system
/// - no permission to read destination file
fn config_file_target_permissions(
    file_entry: &FileEntry,
    dest: &Utf8Path,
) -> anyhow::Result<Permissions> {
    let current_file_permissions = match std::fs::metadata(dest) {
        Err(err) => match err.kind() {
            ErrorKind::PermissionDenied => return Err(err).context("no permissions"),
            ErrorKind::NotFound => None,
            _ => return Err(err).context("unexpected IO error"),
        },
        Ok(p) => Some(p),
    };

    let entry_uid = if let Some(ref u) = file_entry.file_permissions.user {
        let uid = uzers::get_user_by_name(u)
            .with_context(|| format!("no such user: '{u}'"))?
            .uid();
        Some(uid)
    } else {
        None
    };

    let entry_gid = if let Some(ref g) = file_entry.file_permissions.group {
        let gid = uzers::get_group_by_name(g)
            .with_context(|| format!("no such group: '{g}'"))?
            .gid();
        Some(gid)
    } else {
        None
    };
    let entry_mode = file_entry.file_permissions.mode;

    let uid = current_file_permissions
        .as_ref()
        .map(|p| p.uid())
        .or(entry_uid)
        .unwrap_or(0);

    let gid = current_file_permissions
        .as_ref()
        .map(|p| p.gid())
        .or(entry_gid)
        .unwrap_or(0);

    let mode = current_file_permissions
        .as_ref()
        .map(|p| p.mode())
        .or(entry_mode)
        .unwrap_or(0o644);

    Ok(Permissions { uid, gid, mode })
}

struct Permissions {
    uid: u32,
    gid: u32,
    mode: u32,
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
