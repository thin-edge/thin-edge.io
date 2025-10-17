mod actor;
mod config;
mod error;
mod plugin;
mod plugin_manager;

#[cfg(test)]
mod tests;

use crate::plugin_manager::ExternalPlugins;
pub use actor::*;
pub use config::*;
use std::path::PathBuf;
use std::vec;
use tedge_actors::Builder;
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
use tedge_api::commands::LogUploadCmd;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandData;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::OperationName;
use tedge_api::workflow::SyncOnCommand;
use tedge_file_system_ext::FsWatchEvent;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::move_file;
use tedge_utils::file::FileError;
use tedge_utils::file::PermissionEntry;
use tedge_utils::fs::atomically_write_file_sync;
use tedge_utils::fs::AtomFileError;
use toml::toml;

#[cfg(test)]
use tedge_api::Jsonify;
#[cfg(test)]
use tedge_mqtt_ext::*;
#[cfg(test)]
use tracing::error;

/// This is an actor builder.
pub struct LogManagerBuilder {
    config: LogManagerConfig,
    box_builder: SimpleMessageBoxBuilder<LogInput, LogOutput>,
    upload_sender: DynSender<LogUploadRequest>,
}

impl LogManagerBuilder {
    pub async fn try_new(
        config: LogManagerConfig,
        fs_notify: &mut impl MessageSource<FsWatchEvent, Vec<PathBuf>>,
        uploader_actor: &mut impl Service<LogUploadRequest, LogUploadResult>,
    ) -> Result<Self, FileError> {
        Self::init(&config).await?;

        let box_builder = SimpleMessageBoxBuilder::new("Log Manager", 16);
        fs_notify.connect_sink(
            LogManagerBuilder::watched_directories(&config),
            &box_builder.get_sender(),
        );

        let upload_sender = uploader_actor.connect_client(box_builder.get_sender().sender_clone());

        Ok(Self {
            config,
            box_builder,
            upload_sender,
        })
    }

    pub async fn init(config: &LogManagerConfig) -> Result<(), FileError> {
        if config.plugin_config_path.exists() {
            return Ok(());
        }

        // creating plugin config parent dir
        create_directory_with_defaults(&config.plugin_config_dir).await?;

        let legacy_plugin_config = config.config_dir.join("c8y").join("c8y-log-plugin.toml");
        if legacy_plugin_config.exists() {
            move_file(
                legacy_plugin_config,
                &config.plugin_config_path,
                PermissionEntry::default(),
            )
            .await?;
            return Ok(());
        }

        // creating tedge-log-plugin.toml
        let agent_logs_path = format!("{}/agent/workflow-software_*", config.log_dir);
        let example_config = toml! {
            [[files]]
            type = "software-management"
            path = agent_logs_path
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

    /// Directories watched by the log actor
    /// - for configuration changes
    /// - for plugin changes
    fn watched_directories(config: &LogManagerConfig) -> Vec<PathBuf> {
        let mut watch_dirs = vec![config.plugin_config_dir.clone()];
        for dir in &config.plugin_dirs {
            watch_dirs.push(dir.into());
        }
        watch_dirs
    }
}

#[cfg(test)]
impl LogManagerBuilder {
    pub(crate) fn connect_mqtt(
        &mut self,
        mqtt: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
    ) {
        mqtt.connect_mapped_source(
            NoConfig,
            &mut self.box_builder,
            Self::mqtt_message_builder(&self.config),
        );
        self.box_builder.connect_mapped_source(
            Self::subscriptions(&self.config),
            mqtt,
            Self::mqtt_message_parser(&self.config),
        );
    }

    /// List of MQTT topic filters the log actor has to subscribe to
    fn subscriptions(config: &LogManagerConfig) -> TopicFilter {
        let mut topics = config.logfile_request_topic.clone();
        topics.add_all(config.log_metadata_sync_topics.clone());
        topics
    }

    /// Extract a log actor request from an MQTT message
    fn mqtt_message_parser(config: &LogManagerConfig) -> impl Fn(MqttMessage) -> Option<LogInput> {
        let logfile_request_topic = config.logfile_request_topic.clone();
        let log_metadata_sync_topics = config.log_metadata_sync_topics.clone();
        let mqtt_schema = config.mqtt_schema.clone();
        move |message| {
            if logfile_request_topic.accept(&message) {
                LogUploadCmd::parse(&mqtt_schema, message)
                    .map_err(|err| {
                        error!(
                            target: "log plugins",
                            "Incorrect log request payload: {}", err
                        )
                    })
                    .unwrap_or(None)
                    .map(|cmd| cmd.into())
            } else if log_metadata_sync_topics.accept(&message) {
                if let Ok(cmd) = GenericCommandState::from_command_message(&message) {
                    if cmd.is_finished() {
                        return Some(LogInput::CmdMetaSyncSignal(()));
                    }
                }
                None
            } else {
                error!(
                    target: "log plugins",
                    "Received unexpected message on topic: {}", message.topic.name
                );
                None
            }
        }
    }

    /// Build an MQTT message from a log actor response
    fn mqtt_message_builder(
        config: &LogManagerConfig,
    ) -> impl Fn(LogOutput) -> Option<MqttMessage> {
        let metadata_topic = config.logtype_reload_topic.clone();
        let mqtt_schema = config.mqtt_schema.clone();
        move |res| {
            let msg = match res {
                LogOutput::LogUploadCmd(state) => state.command_message(&mqtt_schema),
                LogOutput::LogUploadCmdMetadata(metadata) => {
                    MqttMessage::new(&metadata_topic, metadata.to_bytes())
                        .with_retain()
                        .with_qos(QoS::AtLeastOnce)
                }
            };
            Some(msg)
        }
    }
}

impl RuntimeRequestSink for LogManagerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl Builder<LogManagerActor> for LogManagerBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<LogManagerActor, Self::Error> {
        let message_box = self.box_builder.build();

        let external_plugins = ExternalPlugins::new(
            self.config.plugin_dirs.clone(),
            self.config.sudo_enabled,
            self.config.tmp_dir.clone(),
        );

        Ok(LogManagerActor::new(
            self.config,
            message_box,
            self.upload_sender,
            external_plugins,
        ))
    }
}

impl MessageSource<GenericCommandData, NoConfig> for LogManagerBuilder {
    fn connect_sink(&mut self, config: NoConfig, peer: &impl MessageSink<GenericCommandData>) {
        self.box_builder
            .connect_mapped_sink(config, &peer.get_sender(), |msg: LogOutput| {
                msg.into_generic_command()
            })
    }
}

impl IntoIterator for &LogManagerBuilder {
    type Item = (OperationName, DynSender<GenericCommandState>);
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let sender =
            MappingSender::new(self.box_builder.get_sender(), |cmd: GenericCommandState| {
                LogUploadCmd::try_from(cmd).map(LogInput::LogUploadCmd).ok()
            });
        vec![(OperationType::LogUpload.to_string(), sender.into())].into_iter()
    }
}

impl MessageSink<CmdMetaSyncSignal> for LogManagerBuilder {
    fn get_sender(&self) -> DynSender<CmdMetaSyncSignal> {
        self.box_builder.get_sender().sender_clone()
    }
}

impl SyncOnCommand for LogManagerBuilder {
    /// Return the list of operations for which this actor wants to receive sync signals
    fn sync_on_commands(&self) -> Vec<OperationType> {
        vec![OperationType::SoftwareUpdate]
    }
}
