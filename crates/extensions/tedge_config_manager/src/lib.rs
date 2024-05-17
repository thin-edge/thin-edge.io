mod actor;
mod config;
mod error;

#[cfg(test)]
mod tests;

use actor::*;
pub use config::*;
use log::error;
use std::path::PathBuf;
use tedge_actors::Builder;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Service;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::create_file_with_defaults;
use tedge_utils::file::move_file;
use tedge_utils::file::FileError;
use tedge_utils::file::PermissionEntry;
use toml::toml;

/// An instance of the config manager
///
/// This is an actor builder.
pub struct ConfigManagerBuilder {
    config: ConfigManagerConfig,
    plugin_config: PluginConfig,
    box_builder: SimpleMessageBoxBuilder<ConfigInput, ConfigOperationData>,
    download_sender: DynSender<ConfigDownloadRequest>,
    upload_sender: DynSender<ConfigUploadRequest>,
}

impl ConfigManagerBuilder {
    pub async fn try_new(
        config: ConfigManagerConfig,
        mqtt: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
        fs_notify: &mut impl MessageSource<FsWatchEvent, PathBuf>,
        downloader_actor: &mut impl Service<ConfigDownloadRequest, ConfigDownloadResult>,
        uploader_actor: &mut impl Service<ConfigUploadRequest, ConfigUploadResult>,
    ) -> Result<Self, FileError> {
        Self::init(&config).await?;

        let plugin_config = PluginConfig::new(config.plugin_config_path.as_path());
        let mut box_builder = SimpleMessageBoxBuilder::new("Tedge-Config-Manager", 16);

        mqtt.connect_source(NoConfig, &mut box_builder);
        box_builder.connect_mapped_source(
            Self::subscriptions(&config),
            mqtt,
            Self::mqtt_message_parser(&config),
        );

        let download_sender =
            downloader_actor.connect_client(box_builder.get_sender().sender_clone());

        let upload_sender = uploader_actor.connect_client(box_builder.get_sender().sender_clone());

        fs_notify.connect_sink(
            ConfigManagerBuilder::watched_directory(&config),
            &box_builder.get_sender(),
        );

        Ok(ConfigManagerBuilder {
            config,
            plugin_config,
            box_builder,
            download_sender,
            upload_sender,
        })
    }

    pub async fn init(config: &ConfigManagerConfig) -> Result<(), FileError> {
        if config.plugin_config_path.exists() {
            return Ok(());
        }

        // creating plugin config parent dir
        create_directory_with_defaults(&config.plugin_config_dir)?;

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
            mode = 444
        }
        .to_string();
        create_file_with_defaults(&config.plugin_config_path, Some(&example_config))?;

        Ok(())
    }

    /// List of MQTT topic filters the log actor has to subscribe to
    fn subscriptions(config: &ConfigManagerConfig) -> TopicFilter {
        let mut topic_filter = TopicFilter::empty();
        topic_filter.add_all(config.config_snapshot_topic.clone());
        if config.config_update_enabled {
            topic_filter.add_all(config.config_update_topic.clone());
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

        Ok(ConfigManagerActor::new(
            self.config,
            self.plugin_config,
            input_receiver,
            output_sender,
            self.download_sender,
            self.upload_sender,
        ))
    }
}
