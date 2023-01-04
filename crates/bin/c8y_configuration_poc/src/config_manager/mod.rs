mod actor;
mod config;
mod config_manager;
// mod download;
mod error;
mod plugin_config;
mod upload;

use crate::c8y_http_proxy::handle::C8YHttpProxy;
use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResponse;
use crate::c8y_http_proxy::C8YHttpProxyBuilder;
use crate::file_system_ext::FsWatchActorBuilder;
use crate::mqtt_ext::*;
use actor::*;
use async_trait::async_trait;
pub use config::*;
use tedge_actors::mpsc;
use tedge_actors::ActorBuilder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::PeerLinker;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;

use self::config_manager::ConfigManager;

/// An instance of the config manager
///
/// This is an actor builder.
pub struct ConfigManagerBuilder {
    config: ConfigManagerConfig,
    events_receiver: mpsc::Receiver<ConfigInput>,
    http_responses_receiver: mpsc::Receiver<C8YRestResponse>,
    events_sender: mpsc::Sender<ConfigInput>,
    mqtt_publisher: Option<DynSender<MqttMessage>>,
    http_responses_sender: mpsc::Sender<C8YRestResponse>,
    http_requests_sender: Option<DynSender<C8YRestRequest>>,
    c8y_http_proxy: Option<C8YHttpProxy>,
}

impl ConfigManagerBuilder {
    pub fn new(config: ConfigManagerConfig) -> ConfigManagerBuilder {
        let (events_sender, events_receiver) = mpsc::channel(10);
        let (http_responses_sender, http_responses_receiver) = mpsc::channel(1);

        ConfigManagerBuilder {
            config,
            events_receiver,
            http_responses_receiver,
            events_sender,
            mqtt_publisher: None,
            http_responses_sender,
            http_requests_sender: None,
            c8y_http_proxy: None,
        }
    }

    /// Connect this config manager instance to some http connection provider
    pub fn with_c8y_http_proxy(&mut self, http: &mut C8YHttpProxyBuilder) -> Result<(), LinkError> {
        let http_requests_sender = http.connect(self.http_responses_sender.clone().into())?;
        self.http_requests_sender = Some(http_requests_sender);
        self.c8y_http_proxy = Some(http.handle());
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

        let mqtt_publisher = mqtt.add_client(subscriptions, self.events_sender.clone().into())?;

        self.mqtt_publisher = Some(mqtt_publisher);
        Ok(())
    }

    pub fn with_fs_connection(
        &mut self,
        fs_builder: &mut FsWatchActorBuilder,
    ) -> Result<(), LinkError> {
        let config_dir = self.config.config_dir.clone();
        fs_builder.new_watcher(config_dir, self.events_sender.clone().into());

        Ok(())
    }
}

#[async_trait]
impl ActorBuilder for ConfigManagerBuilder {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let mqtt_con = self.mqtt_publisher.ok_or_else(|| LinkError::MissingPeer {
            role: "mqtt".to_string(),
        })?;

        let http_con = self
            .http_requests_sender
            .ok_or_else(|| LinkError::MissingPeer {
                role: "http".to_string(),
            })?;

        let peers = ConfigManagerMessageBox::new(
            self.events_receiver,
            self.http_responses_receiver,
            http_con,
            mqtt_con.clone(),
        );

        let config_manager =
            ConfigManager::new(self.config.clone(), mqtt_con, self.c8y_http_proxy.unwrap())
                .await
                .unwrap();

        let actor = ConfigManagerActor { config_manager };

        runtime.run(actor, peers).await?;
        Ok(())
    }
}
