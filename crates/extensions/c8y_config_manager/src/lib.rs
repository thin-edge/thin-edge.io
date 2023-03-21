mod actor;
mod child_device;
mod config;
mod download;
mod error;
mod plugin_config;
mod upload;

#[cfg(test)]
mod tests;

use self::child_device::ChildConfigOperationKey;
use actor::*;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
pub use config::*;
use error::ConfigManagementError;
use std::path::Path;
use std::path::PathBuf;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::LoggingReceiver;
use tedge_actors::LoggingSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceConsumer;
use tedge_actors::ServiceProvider;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::*;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;
use tedge_utils::file::create_directory_with_user_group;
use tedge_utils::file::create_file_with_user_group;

pub fn init(config_dir: &Path) -> Result<(), ConfigManagementError> {
    create_directory_with_user_group(
        format!("{}/c8y", config_dir.display()),
        "root",
        "root",
        0o1777,
    )?;
    let example_config = r#"# Add the configurations to be managed by c8y-configuration-plugin

files = [
#    { path = '/etc/tedge/tedge.toml' },
#    { path = '/etc/tedge/mosquitto-conf/c8y-bridge.conf', type = 'c8y-bridge.conf' },
#    { path = '/etc/tedge/mosquitto-conf/tedge-mosquitto.conf', type = 'tedge-mosquitto.conf' },
#    { path = '/etc/mosquitto/mosquitto.conf', type = 'mosquitto.conf' },
#    { path = '/etc/tedge/c8y/example.txt', type = 'example', user = 'tedge', group = 'tedge', mode = 0o444 }
]"#;

    create_file_with_user_group(
        format!("{}/c8y/c8y-configuration-plugin.toml", config_dir.display()),
        "root",
        "root",
        0o644,
        Some(example_config),
    )?;

    create_directory_with_user_group(
        format!("{}/operations/c8y", config_dir.display()),
        "tedge",
        "tedge",
        0o775,
    )?;
    create_file_with_user_group(
        format!(
            "{}/operations/c8y/c8y_UploadConfigFile",
            config_dir.display()
        ),
        "tedge",
        "tedge",
        0o644,
        None,
    )?;
    create_file_with_user_group(
        format!(
            "{}/operations/c8y/c8y_DownloadConfigFile",
            config_dir.display()
        ),
        "tedge",
        "tedge",
        0o644,
        None,
    )?;
    Ok(())
}

/// An instance of the config manager
///
/// This is an actor builder.
pub struct ConfigManagerBuilder {
    config: ConfigManagerConfig,
    receiver: LoggingReceiver<ConfigInput>,
    events_sender: mpsc::Sender<ConfigInput>,
    mqtt_publisher: Option<DynSender<MqttMessage>>,
    c8y_http_proxy: Option<C8YHttpProxy>,
    timer_sender: Option<DynSender<SetTimeout<ChildConfigOperationKey>>>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl ConfigManagerBuilder {
    pub fn new(config: ConfigManagerConfig) -> ConfigManagerBuilder {
        let (events_sender, events_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let receiver = LoggingReceiver::new(
            "C8Y-Config-Manager".into(),
            events_receiver,
            signal_receiver,
        );

        ConfigManagerBuilder {
            config,
            receiver,
            events_sender,
            mqtt_publisher: None,
            c8y_http_proxy: None,
            timer_sender: None,
            signal_sender,
        }
    }

    /// Connect this config manager instance to some http connection provider
    pub fn with_c8y_http_proxy(
        &mut self,
        http: &mut impl ServiceProvider<C8YRestRequest, C8YRestResult, NoConfig>,
    ) -> Result<(), LinkError> {
        self.c8y_http_proxy = Some(C8YHttpProxy::new("ConfigManager => C8Y", http));
        Ok(())
    }

    /// Connect this config manager instance to some mqtt connection provider
    pub fn with_mqtt_connection<T>(&mut self, mqtt: &mut T) -> Result<(), LinkError>
    where
        T: ServiceProvider<MqttMessage, MqttMessage, TopicFilter>,
    {
        mqtt.add_peer(self);
        Ok(())
    }

    pub fn with_fs_connection<T>(&mut self, fs_builder: &mut T) -> Result<(), LinkError>
    where
        T: MessageSource<FsWatchEvent, PathBuf>,
    {
        let config_dir = self.config.config_dir.clone();
        fs_builder.register_peer(config_dir, self.events_sender.clone().into());

        Ok(())
    }

    pub fn with_timer(
        &mut self,
        timer_builder: &mut impl ServiceProvider<OperationTimer, OperationTimeout, NoConfig>,
    ) -> Result<(), LinkError> {
        timer_builder.add_peer(self);
        Ok(())
    }
}

impl MessageSink<FsWatchEvent> for ConfigManagerBuilder {
    fn get_sender(&self) -> DynSender<FsWatchEvent> {
        self.events_sender.clone().into()
    }
}

impl
    ServiceConsumer<SetTimeout<ChildConfigOperationKey>, Timeout<ChildConfigOperationKey>, NoConfig>
    for ConfigManagerBuilder
{
    fn get_config(&self) -> NoConfig {
        NoConfig
    }

    fn set_request_sender(&mut self, sender: DynSender<SetTimeout<ChildConfigOperationKey>>) {
        self.timer_sender = Some(sender);
    }

    fn get_response_sender(&self) -> DynSender<Timeout<ChildConfigOperationKey>> {
        self.events_sender.clone().into()
    }
}

impl ServiceConsumer<MqttMessage, MqttMessage, TopicFilter> for ConfigManagerBuilder {
    fn get_config(&self) -> TopicFilter {
        vec![
            "c8y/s/ds",
            "tedge/+/commands/res/config_snapshot",
            "tedge/+/commands/res/config_update",
        ]
        .try_into()
        .unwrap()
    }

    fn set_request_sender(&mut self, request_sender: DynSender<MqttMessage>) {
        self.mqtt_publisher = Some(request_sender)
    }

    fn get_response_sender(&self) -> DynSender<MqttMessage> {
        self.events_sender.clone().into()
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
        let mqtt_publisher = self
            .mqtt_publisher
            .ok_or_else(|| LinkError::MissingPeer {
                role: "mqtt".to_string(),
            })
            .map(|mqtt_publisher| {
                LoggingSender::new("ConfigManager MQTT publisher".into(), mqtt_publisher)
            })?;

        let c8y_http_proxy = self.c8y_http_proxy.ok_or_else(|| LinkError::MissingPeer {
            role: "c8y-http".to_string(),
        })?;

        let timer_sender = self
            .timer_sender
            .ok_or_else(|| LinkError::MissingPeer {
                role: "timer".to_string(),
            })
            .map(|timer_sender| LoggingSender::new("ConfigManager timer".into(), timer_sender))?;

        let peers = ConfigManagerMessageBox::new(
            self.receiver,
            mqtt_publisher,
            c8y_http_proxy,
            timer_sender,
        );

        Ok(ConfigManagerActor::new(self.config, peers))
    }
}
