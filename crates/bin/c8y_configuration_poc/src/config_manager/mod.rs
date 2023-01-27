mod actor;
mod config;
mod download;
mod error;
mod plugin_config;
mod upload;

use crate::c8y_http_proxy::handle::C8YHttpProxy;
use crate::c8y_http_proxy::C8YConnectionBuilder;
use crate::file_system_ext::FsWatchActorBuilder;
use crate::file_system_ext::FsWatchEvent;
use actor::*;
use async_trait::async_trait;
pub use config::*;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::ActorBuilder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_mqtt_ext::*;

/// An instance of the config manager
///
/// This is an actor builder.
pub struct ConfigManagerBuilder {
    config: ConfigManagerConfig,
    events_receiver: mpsc::Receiver<ConfigInput>,
    events_sender: mpsc::Sender<ConfigInput>,
    mqtt_publisher: Option<DynSender<MqttMessage>>,
    c8y_http_proxy: Option<C8YHttpProxy>,
}

impl ConfigManagerBuilder {
    pub fn new(config: ConfigManagerConfig) -> ConfigManagerBuilder {
        let (events_sender, events_receiver) = mpsc::channel(10);

        ConfigManagerBuilder {
            config,
            events_receiver,
            events_sender,
            mqtt_publisher: None,
            c8y_http_proxy: None,
        }
    }

    /// Connect this config manager instance to some http connection provider
    pub fn with_c8y_http_proxy(
        &mut self,
        http: &mut impl C8YConnectionBuilder,
    ) -> Result<(), LinkError> {
        // self.connect_to(http, ());
        self.c8y_http_proxy = Some(C8YHttpProxy::new("ConfigManager => C8Y", http));
        Ok(())
    }

    /// Connect this config manager instance to some mqtt connection provider
    pub fn with_mqtt_connection(&mut self, mqtt: &mut MqttActorBuilder) -> Result<(), LinkError> {
        let subscriptions = vec![
            "c8y/s/ds",
            "tedge/+/commands/res/config_snapshot",
            "tedge/+/commands/res/config_update",
        ]
        .try_into()
        .unwrap();

        //Register peers symmetrically here
        mqtt.register_peer(subscriptions, self.events_sender.clone().into());
        self.register_peer(NoConfig, mqtt.get_sender());

        Ok(())
    }

    pub fn with_fs_connection(
        &mut self,
        fs_builder: &mut FsWatchActorBuilder,
    ) -> Result<(), LinkError> {
        let config_dir = self.config.config_dir.clone();
        fs_builder.register_peer(config_dir, self.events_sender.clone().into());

        Ok(())
    }
}

impl MessageSource<MqttMessage, NoConfig> for ConfigManagerBuilder {
    fn register_peer(&mut self, _config: NoConfig, sender: DynSender<MqttMessage>) {
        self.mqtt_publisher = Some(sender);
    }
}

impl MessageSink<MqttMessage> for ConfigManagerBuilder {
    fn get_sender(&self) -> DynSender<MqttMessage> {
        self.events_sender.clone().into()
    }
}

impl MessageSink<FsWatchEvent> for ConfigManagerBuilder {
    fn get_sender(&self) -> DynSender<FsWatchEvent> {
        self.events_sender.clone().into()
    }
}

#[async_trait]
impl ActorBuilder for ConfigManagerBuilder {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let mqtt_publisher = self.mqtt_publisher.ok_or_else(|| LinkError::MissingPeer {
            role: "mqtt".to_string(),
        })?;

        let c8y_http_proxy = self.c8y_http_proxy.ok_or_else(|| LinkError::MissingPeer {
            role: "c8y-http".to_string(),
        })?;

        let peers =
            ConfigManagerMessageBox::new(self.events_receiver, mqtt_publisher, c8y_http_proxy);

        let actor = ConfigManagerActor::new(self.config).await;

        runtime.run(actor, peers).await?;
        Ok(())
    }
}
