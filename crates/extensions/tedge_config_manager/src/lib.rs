mod actor;
mod config;
mod error;
mod plugin;
mod plugin_manager;

#[cfg(test)]
mod tests;

use crate::plugin_manager::ExternalPlugins;
use actor::*;
pub use config::*;
use log::error;
use serde_json::json;
use std::path::PathBuf;
use tedge_actors::Builder;
use tedge_actors::ClientMessageBox;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::MappingSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Service;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::commands::CmdMetaSyncSignal;
use tedge_api::commands::ConfigSnapshotCmd;
use tedge_api::commands::ConfigUpdateCmd;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandData;
use tedge_api::workflow::GenericCommandMetadata;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::OperationName;
use tedge_api::workflow::SyncOnCommand;
use tedge_api::Jsonify;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::move_file;
use tedge_utils::file::FileError;
use tedge_utils::file::PermissionEntry;
use tedge_utils::fs::atomically_write_file_sync;
use tedge_utils::fs::AtomFileError;
use toml::toml;

/// An instance of the config manager
///
/// This is an actor builder.
pub struct ConfigManagerBuilder {
    config: ConfigManagerConfig,
    plugin_config: PluginConfig,
    box_builder: SimpleMessageBoxBuilder<ConfigInput, ConfigOperationData>,
    downloader: ClientMessageBox<ConfigDownloadRequest, ConfigDownloadResult>,
    uploader: ClientMessageBox<ConfigUploadRequest, ConfigUploadResult>,
}

impl ConfigManagerBuilder {
    pub async fn try_new(
        config: ConfigManagerConfig,
        fs_notify: &mut impl MessageSource<FsWatchEvent, PathBuf>,
        downloader_actor: &mut impl Service<ConfigDownloadRequest, ConfigDownloadResult>,
        uploader_actor: &mut impl Service<ConfigUploadRequest, ConfigUploadResult>,
    ) -> Result<Self, FileError> {
        Self::init(&config).await?;

        let plugin_config = PluginConfig::new(config.plugin_config_path.as_path());
        let box_builder = SimpleMessageBoxBuilder::new("Tedge-Config-Manager", 16);

        let downloader = ClientMessageBox::new(downloader_actor);

        let uploader = ClientMessageBox::new(uploader_actor);

        fs_notify.connect_sink(
            ConfigManagerBuilder::watched_directory(&config),
            &box_builder.get_sender(),
        );

        Ok(ConfigManagerBuilder {
            config,
            plugin_config,
            box_builder,
            downloader,
            uploader,
        })
    }

    pub fn connect_mqtt(
        &mut self,
        mqtt: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
    ) {
        mqtt.connect_source(NoConfig, &mut self.box_builder);
        self.box_builder.connect_mapped_source(
            Self::subscriptions(&self.config),
            mqtt,
            Self::mqtt_message_parser(&self.config),
        );
    }

    pub async fn init(config: &ConfigManagerConfig) -> Result<(), FileError> {
        if config.plugin_config_path.exists() {
            return Ok(());
        }

        // creating plugin config parent dir
        create_directory_with_defaults(&config.plugin_config_dir).await?;

        let legacy_plugin_config = config
            .config_dir
            .join("c8y")
            .join("c8y-configuration-plugin.toml");
        if legacy_plugin_config.exists() {
            move_file(
                legacy_plugin_config,
                &config.plugin_config_path,
                PermissionEntry::default(),
            )
            .await?;
            return Ok(());
        }

        // create tedge-configuration-plugin.toml
        let tedge_config_path = format!("{}/tedge.toml", config.config_dir.to_string_lossy());
        let tedge_log_plugin_config_path = format!(
            "{}/plugins/tedge-log-plugin.toml",
            config.config_dir.to_string_lossy()
        );
        let example_config = toml! {
            [[files]]
            path = tedge_config_path
            type = "tedge.toml"

            [[files]]
            path = tedge_log_plugin_config_path
            type = "tedge-log-plugin"
            user = "tedge"
            group = "tedge"
            mode = 0o644
        }
        .to_string();
        atomically_write_file_sync(&config.plugin_config_path, example_config.as_bytes()).map_err(
            |AtomFileError::WriteError { source, .. }| FileError::FileCreateFailed {
                file: config.plugin_config_path.to_string_lossy().to_string(),
                from: source,
            },
        )?;

        Ok(())
    }

    /// List of MQTT topic filters the log actor has to subscribe to
    fn subscriptions(config: &ConfigManagerConfig) -> TopicFilter {
        let mut topic_filter = config.config_snapshot_topic.clone();
        if config.config_update_enabled {
            topic_filter += config.config_update_topic.clone();
        }
        topic_filter
    }

    /// Directory watched by the config actors for configuration changes
    fn watched_directory(config: &ConfigManagerConfig) -> PathBuf {
        config.plugin_config_dir.clone()
    }

    /// Extract a config actor request from an MQTT message
    fn mqtt_message_parser(
        config: &ConfigManagerConfig,
    ) -> impl Fn(MqttMessage) -> Option<ConfigInput> {
        let config = config.clone();
        move |message| match ConfigOperation::request_from_message(&config, &message) {
            Ok(Some(request)) => Some(request.into()),
            Ok(None) => None,
            Err(err) => {
                error!("Received invalid config request: {err}");
                None
            }
        }
    }
}

impl RuntimeRequestSink for ConfigManagerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl Builder<ConfigManagerActor> for ConfigManagerBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<ConfigManagerActor, Self::Error> {
        let (output_sender, input_receiver) = self.box_builder.build().into_split();

        let external_plugins = ExternalPlugins::new(
            self.config.plugin_dirs.clone(),
            true,
            self.config.tmp_path.clone(),
        );

        Ok(ConfigManagerActor::new(
            self.config,
            self.plugin_config,
            input_receiver,
            output_sender,
            self.downloader,
            self.uploader,
            external_plugins,
        ))
    }
}

impl MessageSource<GenericCommandData, NoConfig> for ConfigManagerBuilder {
    fn connect_sink(&mut self, config: NoConfig, peer: &impl MessageSink<GenericCommandData>) {
        self.box_builder.connect_mapped_sink(
            config,
            &peer.get_sender(),
            |data: ConfigOperationData| match data {
                ConfigOperationData::State(ConfigOperation::Snapshot(topic, payload)) => Some(
                    GenericCommandState::new(topic, payload.status.to_string(), payload.to_value())
                        .into(),
                ),
                ConfigOperationData::State(ConfigOperation::Update(topic, payload)) => Some(
                    GenericCommandState::new(topic, payload.status.to_string(), payload.to_value())
                        .into(),
                ),
                ConfigOperationData::Metadata { topic, types } => {
                    let operation = MqttSchema::get_operation_name(topic.as_ref())?;
                    Some(GenericCommandData::Metadata(GenericCommandMetadata {
                        operation,
                        payload: json!( {
                            "types": types
                        }),
                    }))
                }
            },
        )
    }
}

impl IntoIterator for &ConfigManagerBuilder {
    type Item = (OperationName, DynSender<GenericCommandState>);
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let mut operation_senders = vec![(
            OperationType::ConfigSnapshot.to_string(),
            MappingSender::new(
                self.box_builder.get_sender(),
                generic_command_into_snapshot_request,
            )
            .into(),
        )];
        if self.config.config_update_enabled {
            operation_senders.push((
                OperationType::ConfigUpdate.to_string(),
                MappingSender::new(
                    self.box_builder.get_sender(),
                    generic_command_into_update_request,
                )
                .into(),
            ));
        }
        operation_senders.into_iter()
    }
}

fn generic_command_into_snapshot_request(cmd: GenericCommandState) -> Option<ConfigInput> {
    let topic = cmd.topic.clone();
    let cmd = ConfigSnapshotCmd::try_from(cmd).ok()?;
    Some(ConfigOperation::Snapshot(topic, cmd.payload).into())
}

fn generic_command_into_update_request(cmd: GenericCommandState) -> Option<ConfigInput> {
    let topic = cmd.topic.clone();
    let cmd = ConfigUpdateCmd::try_from(cmd).ok()?;
    Some(ConfigOperation::Update(topic, cmd.payload).into())
}

impl MessageSink<CmdMetaSyncSignal> for ConfigManagerBuilder {
    fn get_sender(&self) -> DynSender<CmdMetaSyncSignal> {
        self.box_builder.get_sender().sender_clone()
    }
}

impl SyncOnCommand for ConfigManagerBuilder {
    /// Return the list of operations for which this actor wants to receive sync signals
    fn sync_on_commands(&self) -> Vec<OperationType> {
        vec![OperationType::SoftwareUpdate]
    }
}
