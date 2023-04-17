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
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::*;

/// This is an actor builder.
pub struct LogManagerBuilder {
    config: LogManagerConfig,
    box_builder: SimpleMessageBoxBuilder<LogInput, NoMessage>,
    mqtt_publisher: DynSender<MqttMessage>,
    http_proxy: C8YHttpProxy,
}

impl LogManagerBuilder {
    pub fn new(
        config: LogManagerConfig,
        mqtt: &mut impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter>,
        http: &mut impl ServiceProvider<C8YRestRequest, C8YRestResult, NoConfig>,
        fs_notify: &mut impl MessageSource<FsWatchEvent, PathBuf>,
    ) -> Self {
        let box_builder = SimpleMessageBoxBuilder::new("C8Y Log Manager", 16);
        let http_proxy = C8YHttpProxy::new("LogManager => C8Y", http);
        let mqtt_publisher = mqtt.connect_consumer(
            LogManagerBuilder::subscriptions(),
            adapt(&box_builder.get_sender()),
        );
        fs_notify.register_peer(
            LogManagerBuilder::watched_directory(&config),
            adapt(&box_builder.get_sender()),
        );

        Self {
            config,
            box_builder,
            mqtt_publisher,
            http_proxy,
        }
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

    /// Directory watched by the log actors for configuration changes
    fn watched_directory(config: &LogManagerConfig) -> PathBuf {
        config.config_dir.clone()
    }
}

impl RuntimeRequestSink for LogManagerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl Builder<LogManagerActor> for LogManagerBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<LogManagerActor, Self::Error> {
        let mqtt_publisher = LoggingSender::new("C8Y-Log-Manager".into(), self.mqtt_publisher);
        let message_box = self.box_builder.build();

        Ok(LogManagerActor::new(
            self.config,
            mqtt_publisher,
            self.http_proxy,
            message_box,
        ))
    }
}
