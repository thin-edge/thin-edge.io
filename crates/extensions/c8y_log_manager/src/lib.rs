mod actor;
mod config;
mod error;

use actor::*;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
pub use config::*;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Builder;
use tedge_actors::CombinedReceiver;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceProvider;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::*;

/// This is an actor builder.
pub struct LogManagerBuilder {
    config: LogManagerConfig,
    input_receiver: CombinedReceiver<LogInput>,
    events_sender: mpsc::Sender<LogInput>,
    mqtt_publisher: Option<DynSender<MqttMessage>>,
    http_proxy: Option<C8YHttpProxy>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl LogManagerBuilder {
    pub fn new(config: LogManagerConfig) -> Self {
        let (events_sender, events_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let input_receiver =
            CombinedReceiver::new("LogManager".into(), events_receiver, signal_receiver);

        Self {
            config,
            input_receiver,
            events_sender,
            mqtt_publisher: None,
            http_proxy: None,
            signal_sender,
        }
    }

    /// Connect this config manager instance to some http connection provider
    pub fn with_c8y_http_proxy(
        &mut self,
        http: &mut impl ServiceProvider<C8YRestRequest, C8YRestResult, NoConfig>,
    ) -> Result<(), LinkError> {
        self.http_proxy = Some(C8YHttpProxy::new("LogManager => C8Y", http));
        Ok(())
    }

    /// Connect this config manager instance to some mqtt connection provider
    pub fn with_mqtt_connection(&mut self, mqtt: &mut MqttActorBuilder) -> Result<(), LinkError> {
        let subscriptions = vec!["c8y/s/ds"].try_into().unwrap();
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

impl MessageSource<MqttMessage, NoConfig> for LogManagerBuilder {
    fn register_peer(&mut self, _config: NoConfig, sender: DynSender<MqttMessage>) {
        self.mqtt_publisher = Some(sender);
    }
}

impl MessageSink<MqttMessage> for LogManagerBuilder {
    fn get_sender(&self) -> DynSender<MqttMessage> {
        self.events_sender.clone().into()
    }
}

impl MessageSink<FsWatchEvent> for LogManagerBuilder {
    fn get_sender(&self) -> DynSender<FsWatchEvent> {
        self.events_sender.clone().into()
    }
}

impl RuntimeRequestSink for LogManagerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.signal_sender.clone())
    }
}

impl Builder<(LogManagerActor, LogManagerMessageBox)> for LogManagerBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<(LogManagerActor, LogManagerMessageBox), Self::Error> {
        let mqtt_publisher = self.mqtt_publisher.ok_or_else(|| LinkError::MissingPeer {
            role: "mqtt".to_string(),
        })?;

        let http_proxy = self.http_proxy.ok_or_else(|| LinkError::MissingPeer {
            role: "http".to_string(),
        })?;

        let message_box = LogManagerMessageBox::new(self.input_receiver, mqtt_publisher.clone());

        let actor = LogManagerActor::new(self.config, mqtt_publisher, http_proxy);

        Ok((actor, message_box))
    }
}
