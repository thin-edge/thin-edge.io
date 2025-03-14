mod actor;
mod config;
mod error;
mod manager;

#[cfg(test)]
mod tests;

pub use actor::*;
pub use config::*;
use log::error;
use manager::LogPluginConfig;
use std::path::PathBuf;
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
use tedge_api::commands::LogUploadCmd;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandData;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::OperationName;
use tedge_api::Jsonify;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::*;
use tedge_utils::file_async::create_directory_with_defaults;
use tedge_utils::file_async::create_file_with_defaults;
use tedge_utils::file_async::move_file;
use tedge_utils::file_async::FileError;
use tedge_utils::file_async::PermissionEntry;
use toml::toml;

/// This is an actor builder.
pub struct LogManagerBuilder {
    config: LogManagerConfig,
    plugin_config: LogPluginConfig,
    box_builder: SimpleMessageBoxBuilder<LogInput, LogOutput>,
    upload_sender: DynSender<LogUploadRequest>,
}

impl LogManagerBuilder {
    pub async fn try_new(
        config: LogManagerConfig,
        fs_notify: &mut impl MessageSource<FsWatchEvent, PathBuf>,
        uploader_actor: &mut impl Service<LogUploadRequest, LogUploadResult>,
    ) -> Result<Self, FileError> {
        Self::init(&config).await?;
        let plugin_config = LogPluginConfig::new(&config.plugin_config_path);

        let box_builder = SimpleMessageBoxBuilder::new("Log Manager", 16);
        fs_notify.connect_sink(
            LogManagerBuilder::watched_directory(&config),
            &box_builder.get_sender(),
        );

        let upload_sender = uploader_actor.connect_client(box_builder.get_sender().sender_clone());

        Ok(Self {
            config,
            plugin_config,
            box_builder,
            upload_sender,
        })
    }

    pub fn connect_mqtt(
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
        create_file_with_defaults(&config.plugin_config_path, Some(&example_config)).await?;

        Ok(())
    }

    /// List of MQTT topic filters the log actor has to subscribe to
    fn subscriptions(config: &LogManagerConfig) -> TopicFilter {
        config.logfile_request_topic.clone()
    }

    /// Extract a log actor request from an MQTT message
    fn mqtt_message_parser(config: &LogManagerConfig) -> impl Fn(MqttMessage) -> Option<LogInput> {
        let logfile_request_topic = config.logfile_request_topic.clone();
        let mqtt_schema = config.mqtt_schema.clone();
        move |message| {
            if !logfile_request_topic.accept(&message) {
                error!(
                    "Received unexpected message on topic: {}",
                    message.topic.name
                );
                return None;
            }

            LogUploadCmd::parse(&mqtt_schema, message)
                .map_err(|err| error!("Incorrect log request payload: {}", err))
                .unwrap_or(None)
                .map(|cmd| cmd.into())
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

    /// Directory watched by the log actors for configuration changes
    fn watched_directory(config: &LogManagerConfig) -> PathBuf {
        config.plugin_config_dir.clone()
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

        Ok(LogManagerActor::new(
            self.config,
            self.plugin_config,
            message_box,
            self.upload_sender,
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
