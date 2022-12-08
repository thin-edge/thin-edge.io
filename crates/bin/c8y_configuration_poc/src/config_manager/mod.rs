mod actor;
mod config;

use crate::mqtt_ext::*;
use crate::{file_system_ext, mqtt_ext};
use actor::*;
use async_trait::async_trait;
pub use config::*;
use tedge_actors::{ActorBuilder, LinkError, PeerLinker, Recipient, RuntimeError, RuntimeHandle};
use tedge_http_ext::*;

/// An instance of the config manager
///
/// This is an actor builder.
pub struct ConfigManager {
    config: ConfigConfigManager,
    mailbox: ConfigManagerMailbox,
    address: ConfigManagerAddress,
    http_con: Option<Recipient<HttpRequest>>,
}

impl ConfigManager {
    pub fn new(config: ConfigConfigManager) -> ConfigManager {
        let (mailbox, address) = new_config_mailbox();

        ConfigManager {
            config,
            mailbox,
            address,
            http_con: None,
        }
    }

    /// Connect this config manager instance to some http connection provider
    pub fn with_http_connection(&mut self, http: &mut impl PeerLinker<HttpRequest, HttpResult>) -> Result<(), LinkError> {
        let http_con = http.connect(self.address.http_responses.as_recipient())?;
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
            self.address.events.as_recipient(),
        )
        .await?;

        let file_watcher = file_system_ext::new_watcher(
            runtime,
            watcher_config,
            self.address.events.as_recipient(),
        )
        .await?;

        let http_con = self.http_con.ok_or_else(|| LinkError::MissingPeer {role: "http".to_string()})?;
        let peers = ConfigManagerPeers::new(file_watcher, http_con, mqtt_con);

        runtime.run(actor, self.mailbox, peers).await?;
        Ok(())
    }
}
