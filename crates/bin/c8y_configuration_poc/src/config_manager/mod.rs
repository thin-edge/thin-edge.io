mod actor;
mod config;
mod download;
mod error;
mod plugin_config;
mod upload;

use crate::c8y_http_proxy::handle::C8YHttpProxy;
use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResult;
use crate::c8y_http_proxy::C8YConnectionBuilder;
use crate::file_system_ext::FsWatchActorBuilder;
use actor::*;
use async_trait::async_trait;
pub use config::*;
use tedge_actors::mpsc;
use tedge_actors::ActorBuilder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::MessageBoxPlug;
use tedge_actors::MessageBoxSocket;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_actors::Sender;
use tedge_mqtt_ext::*;

/// An instance of the config manager
///
/// This is an actor builder.
pub struct ConfigManagerBuilder {
    config: ConfigManagerConfig,
    events_receiver: mpsc::Receiver<ConfigInput>,
    http_responses_receiver: mpsc::Receiver<C8YRestResult>,
    events_sender: mpsc::Sender<ConfigInput>,
    mqtt_publisher: Option<DynSender<MqttMessage>>,
    http_responses_sender: mpsc::Sender<C8YRestResult>,
    http_requests_sender: Option<DynSender<C8YRestRequest>>,
    c8y_upload_http_proxy: Option<C8YHttpProxy>,
    c8y_download_http_proxy: Option<C8YHttpProxy>,
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
            c8y_upload_http_proxy: None,
            c8y_download_http_proxy: None,
        }
    }

    /// Connect this config manager instance to some http connection provider
    pub fn with_c8y_http_proxy(
        &mut self,
        http: &mut impl C8YConnectionBuilder,
    ) -> Result<(), LinkError> {
        self.connect_to(http, NoConfig);
        self.c8y_upload_http_proxy = Some(C8YHttpProxy::new("UploadManager => C8Y", http));
        self.c8y_download_http_proxy = Some(C8YHttpProxy::new("DownloadManager => C8Y", http));
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

        mqtt.connect_with(self, subscriptions);
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

impl MessageBoxPlug<C8YRestRequest, C8YRestResult> for ConfigManagerBuilder {
    fn set_request_sender(&mut self, request_sender: DynSender<C8YRestRequest>) {
        self.http_requests_sender = Some(request_sender)
    }

    fn get_response_sender(&self) -> DynSender<C8YRestResult> {
        self.http_responses_sender.sender_clone()
    }
}

impl MessageBoxPlug<MqttMessage, MqttMessage> for ConfigManagerBuilder {
    fn set_request_sender(&mut self, mqtt_publisher: DynSender<MqttMessage>) {
        self.mqtt_publisher = Some(mqtt_publisher);
    }

    fn get_response_sender(&self) -> DynSender<MqttMessage> {
        self.events_sender.sender_clone()
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

        let actor = ConfigManagerActor::new(
            self.config.clone(),
            mqtt_con,
            self.c8y_upload_http_proxy.unwrap(),
            self.c8y_download_http_proxy.unwrap(),
        )
        .await;

        runtime.run(actor, peers).await?;
        Ok(())
    }
}
