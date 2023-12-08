mod actor;
mod config;
mod error;

#[cfg(test)]
mod tests;

use actor::*;
pub use config::*;
use std::path::PathBuf;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::LoggingReceiver;
use tedge_actors::LoggingSender;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceProvider;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::create_file_with_defaults;
use tedge_utils::file::move_file;
use tedge_utils::file::FileError;
use tedge_utils::file::PermissionEntry;

/// An instance of the config manager
///
/// This is an actor builder.
pub struct ConfigManagerBuilder {
    config: ConfigManagerConfig,
    plugin_config: PluginConfig,
    receiver: LoggingReceiver<ConfigInput>,
    mqtt_publisher: DynSender<MqttMessage>,
    download_sender: DynSender<ConfigDownloadRequest>,
    upload_sender: DynSender<ConfigUploadRequest>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl ConfigManagerBuilder {
    pub async fn try_new(
        config: ConfigManagerConfig,
        mqtt: &mut impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter>,
        fs_notify: &mut impl MessageSource<FsWatchEvent, PathBuf>,
        downloader_actor: &mut impl ServiceProvider<
            ConfigDownloadRequest,
            ConfigDownloadResult,
            NoConfig,
        >,
        uploader_actor: &mut impl ServiceProvider<ConfigUploadRequest, ConfigUploadResult, NoConfig>,
    ) -> Result<Self, FileError> {
        Self::init(&config).await?;

        let plugin_config = PluginConfig::new(config.plugin_config_path.as_path());

        let (events_sender, events_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let receiver = LoggingReceiver::new(
            "Tedge-Config-Manager".into(),
            events_receiver,
            signal_receiver,
        );

        let mqtt_publisher =
            mqtt.connect_consumer(Self::subscriptions(&config), events_sender.clone().into());

        let download_sender =
            downloader_actor.connect_consumer(NoConfig, events_sender.clone().into());

        let upload_sender = uploader_actor.connect_consumer(NoConfig, events_sender.clone().into());

        fs_notify.register_peer(
            ConfigManagerBuilder::watched_directory(&config),
            events_sender.into(),
        );

        Ok(ConfigManagerBuilder {
            config,
            plugin_config,
            receiver,
            mqtt_publisher,
            download_sender,
            upload_sender,
            signal_sender,
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
        let example_config = r#"# Add the configurations to be managed
files = [
#    { path = '/etc/tedge/tedge.toml' },
#    { path = '/etc/tedge/mosquitto-conf/c8y-bridge.conf', type = 'c8y-bridge.conf' },
#    { path = '/etc/tedge/mosquitto-conf/tedge-mosquitto.conf', type = 'tedge-mosquitto.conf' },
#    { path = '/etc/mosquitto/mosquitto.conf', type = 'mosquitto.conf' },
#    { path = '/etc/tedge/plugins/example.txt', type = 'example', user = 'tedge', group = 'tedge', mode = 0o444 },
]"#;
        create_file_with_defaults(&config.plugin_config_path, Some(example_config))?;

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
}

impl RuntimeRequestSink for ConfigManagerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.signal_sender.clone())
    }
}

impl Builder<ConfigManagerActor> for ConfigManagerBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<ConfigManagerActor, Self::Error> {
        let mqtt_publisher = LoggingSender::new("Tedge-Config-Manager".into(), self.mqtt_publisher);

        Ok(ConfigManagerActor::new(
            self.config,
            self.plugin_config,
            self.receiver,
            mqtt_publisher,
            self.download_sender,
            self.upload_sender,
        ))
    }
}
