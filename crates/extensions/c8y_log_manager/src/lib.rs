mod actor;
mod config;
mod error;

use actor::*;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_api::utils::bridge::C8Y_BRIDGE_HEALTH_TOPIC;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
pub use config::*;
use std::path::PathBuf;
use tedge_actors::adapt;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::LoggingSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::NoMessage;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceConsumer;
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::*;

/// This is an actor builder.
pub struct LogManagerBuilder {
    config: LogManagerConfig,
    box_builder: SimpleMessageBoxBuilder<LogInput, NoMessage>,
    mqtt_publisher: Option<DynSender<MqttMessage>>,
    http_proxy: Option<C8YHttpProxy>,
}

impl LogManagerBuilder {
    pub fn new(config: LogManagerConfig) -> Self {
        let box_builder = SimpleMessageBoxBuilder::new("C8Y Log Manager", 16);

        Self {
            config,
            box_builder,
            mqtt_publisher: None,
            http_proxy: None,
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
    pub fn with_mqtt_connection(
        &mut self,
        mqtt: &mut impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter>,
    ) -> Result<(), LinkError> {
        self.set_connection(mqtt);
        Ok(())
    }

    pub fn with_fs_connection(
        &mut self,
        fs_builder: &mut impl MessageSource<FsWatchEvent, PathBuf>,
    ) -> Result<(), LinkError> {
        let config_dir = self.config.config_dir.clone();
        fs_builder.register_peer(
            config_dir,
            tedge_actors::adapt(&self.box_builder.get_sender()),
        );

        Ok(())
    }

    /// List of MQTT topic filters the log actor has to subscribe to
    fn subscriptions() -> TopicFilter {
        vec![
            // subscribing to c8y smartrest requests
            C8yTopic::SmartRestRequest.to_string().as_ref(),
            // subscribing also to c8y bridge health topic to know when the bridge is up
            C8Y_BRIDGE_HEALTH_TOPIC,
        ]
        .try_into()
        .expect("Well-formed topic filters")
    }
}

impl ServiceConsumer<MqttMessage, MqttMessage, TopicFilter> for LogManagerBuilder {
    fn get_config(&self) -> TopicFilter {
        LogManagerBuilder::subscriptions()
    }

    fn set_request_sender(&mut self, sender: DynSender<MqttMessage>) {
        self.mqtt_publisher = Some(sender);
    }

    fn get_response_sender(&self) -> DynSender<MqttMessage> {
        adapt(&self.box_builder.get_sender())
    }
}

impl MessageSink<FsWatchEvent> for LogManagerBuilder {
    fn get_sender(&self) -> DynSender<FsWatchEvent> {
        tedge_actors::adapt(&self.box_builder.get_sender())
    }
}

impl RuntimeRequestSink for LogManagerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl Builder<(LogManagerActor, SimpleMessageBox<LogInput, NoMessage>)> for LogManagerBuilder {
    type Error = LinkError;

    fn try_build(
        self,
    ) -> Result<(LogManagerActor, SimpleMessageBox<LogInput, NoMessage>), Self::Error> {
        let mqtt_publisher = self
            .mqtt_publisher
            .ok_or_else(|| LinkError::MissingPeer {
                role: "mqtt".into(),
            })
            .map(|mqtt_publisher| LoggingSender::new("C8Y-Log-Manager".into(), mqtt_publisher))?;

        let http_proxy = self.http_proxy.ok_or_else(|| LinkError::MissingPeer {
            role: "http".to_string(),
        })?;

        let message_box = self.box_builder.build();

        let actor = LogManagerActor::new(self.config, mqtt_publisher, http_proxy);

        Ok((actor, message_box))
    }
}
