mod actor;
mod config;

use crate::mqtt_ext::*;
use crate::{file_system_ext, mqtt_ext};
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
pub struct ConfigManager {
    config: ConfigManagerConfig,
    events_receiver: mpsc::Receiver<ConfigInput>,
    http_responses_receiver: mpsc::Receiver<HttpResult>,
    events_sender: mpsc::Sender<ConfigInput>,
    http_responses_sender: mpsc::Sender<HttpResult>,
    http_con: Option<DynSender<HttpRequest>>,
}

impl ConfigManager {
    pub fn new(config: ConfigManagerConfig) -> ConfigManager {
        let (events_sender, events_receiver) = mpsc::channel(10);
        let (http_responses_sender, http_responses_receiver) = mpsc::channel(10);

        ConfigManager {
            config,
            events_receiver,
            http_responses_receiver,
            events_sender,
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
}

#[async_trait]
impl ActorBuilder for ConfigManager {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let actor = ConfigManagerActor {};

        let watcher_config = file_system_ext::WatcherConfig {
            directory: self.config.config_dir,
        };

        let mqtt_con = mqtt_ext::new_connection(
            runtime,
            MqttConfig {
                host: self.config.mqtt_host.to_string(),
                port: self.config.mqtt_port,
            },
            self.events_sender.clone().into(),
        )
        .await?;

        let file_watcher = file_system_ext::new_watcher(
            runtime,
            watcher_config,
            self.events_sender.clone().into(),
        )
        .await?;

        let http_con = self.http_con.ok_or_else(|| LinkError::MissingPeer {
            role: "http".to_string(),
        })?;
        let peers = ConfigManagerPeers::new(
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
