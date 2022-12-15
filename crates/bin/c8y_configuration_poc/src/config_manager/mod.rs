mod actor;
mod config;

use crate::file_system_ext;
use crate::mqtt_ext::*;
use actor::*;
use async_trait::async_trait;
pub use config::*;
use tedge_actors::{
    mpsc, ActorBuilder, DynSender, LinkError, PeerLinker, RuntimeError, RuntimeHandle,
};
use tedge_http_ext::*;

/// An instance of the config manager
///
/// This is an actor builder.
pub struct ConfigManagerBuilder {
    config: ConfigManagerConfig,
    events_receiver: mpsc::Receiver<ConfigInput>,
    http_responses_receiver: mpsc::Receiver<HttpResult>,
    events_sender: mpsc::Sender<ConfigInput>,
    mqtt_publisher: Option<DynSender<MqttMessage>>,
    http_responses_sender: mpsc::Sender<HttpResult>,
    http_con: Option<DynSender<HttpRequest>>,
}

impl ConfigManagerBuilder {
    pub fn new(config: ConfigManagerConfig) -> ConfigManagerBuilder {
        let (events_sender, events_receiver) = mpsc::channel(10);
        let (http_responses_sender, http_responses_receiver) = mpsc::channel(10);

        ConfigManagerBuilder {
            config,
            events_receiver,
            http_responses_receiver,
            events_sender,
            mqtt_publisher: None,
            http_responses_sender,
            http_con: None,
        }
    }

    /// Connect this config manager instance to some http connection provider
    pub fn with_http_connection(
        &mut self,
        http: &mut impl PeerLinker<HttpRequest, HttpResult>,
    ) -> Result<(), LinkError> {
        let http_con = http.connect(self.http_responses_sender.clone().into())?;
        self.http_con = Some(http_con);
        Ok(())
    }

    /// Connect this config manager instance to some mqtt connection provider
    pub fn with_mqtt_connection(&mut self, mqtt: &mut MqttActorBuilder) -> Result<(), LinkError> {
        let subscriptions = vec![
            "c8y/s/us",
            "tedge/+/commands/res/config_snapshot",
            "tedge/+/commands/res/config_update",
        ]
        .try_into()
        .unwrap();

        let mqtt_publisher = mqtt.add_client(subscriptions, self.events_sender.clone().into())?;

        self.mqtt_publisher = Some(mqtt_publisher);
        Ok(())
    }
}

#[async_trait]
impl ActorBuilder for ConfigManagerBuilder {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let actor = ConfigManagerActor {};

        let watcher_config = file_system_ext::WatcherConfig {
            directory: self.config.config_dir,
        };

        let file_watcher = file_system_ext::new_watcher(
            runtime,
            watcher_config,
            self.events_sender.clone().into(),
        )
        .await?;

        let mqtt_con = self.mqtt_publisher.ok_or_else(|| LinkError::MissingPeer {
            role: "mqtt".to_string(),
        })?;

        let http_con = self.http_con.ok_or_else(|| LinkError::MissingPeer {
            role: "http".to_string(),
        })?;

        let peers = ConfigManagerMessageBox::new(
            self.events_receiver,
            self.http_responses_receiver,
            file_watcher,
            http_con,
            mqtt_con,
        );

        runtime.run(actor, peers).await?;
        Ok(())
    }
}
