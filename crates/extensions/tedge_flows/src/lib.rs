mod actor;
mod config;
mod connected_flow;
mod flow;
mod input_source;
mod js_lib;
mod js_runtime;
mod js_script;
mod js_value;
mod registry;
mod runtime;
mod stats;

use crate::actor::FlowsMapper;
use crate::actor::STATS_DUMP_INTERVAL;
use crate::connected_flow::ConnectedFlowRegistry;
pub use crate::flow::*;
pub use crate::registry::BaseFlowRegistry;
pub use crate::registry::FlowRegistryExt;
pub use crate::runtime::MessageProcessor;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::HashSet;
use std::convert::Infallible;
use std::path::PathBuf;
use tedge_actors::fan_in_message_type;
use tedge_actors::Builder;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::NullSender;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::DynSubscriptions;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::MqttRequest;
use tedge_mqtt_ext::SubscriptionDiff;
use tedge_mqtt_ext::TopicFilter;
use tedge_watch_ext::WatchEvent;
use tedge_watch_ext::WatchRequest;
use tokio::time::Instant;

fan_in_message_type!(InputMessage[MqttMessage, WatchEvent, FsWatchEvent, Tick]: Clone, Debug, Eq, PartialEq);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tick;

pub struct FlowsMapperBuilder {
    message_box: SimpleMessageBoxBuilder<InputMessage, SubscriptionDiff>,
    mqtt_sender: DynSender<MqttMessage>,
    watch_request_sender: DynSender<WatchRequest>,
    processor: MessageProcessor<ConnectedFlowRegistry>,
}

impl FlowsMapperBuilder {
    pub async fn try_new(config_dir: impl AsRef<Utf8Path>) -> Result<Self, LoadError> {
        let registry = ConnectedFlowRegistry::new(config_dir);
        let mut processor = MessageProcessor::try_new(registry).await?;
        let message_box = SimpleMessageBoxBuilder::new("TedgeFlows", 16);
        let mqtt_sender = NullSender.into();
        let watch_request_sender = NullSender.into();

        processor.load_all_flows().await;

        Ok(FlowsMapperBuilder {
            message_box,
            mqtt_sender,
            watch_request_sender,
            processor,
        })
    }

    pub fn connect(
        &mut self,
        mqtt: &mut (impl for<'a> MessageSource<MqttMessage, &'a mut DynSubscriptions>
                  + MessageSink<MqttRequest>),
    ) {
        let mut dyn_subscriptions = DynSubscriptions::new(self.topics());
        self.message_box
            .connect_source(&mut dyn_subscriptions, mqtt);
        let client_id = dyn_subscriptions.client_id();
        self.message_box
            .connect_mapped_sink(NoConfig, mqtt, move |diff| {
                Some(MqttRequest::subscribe(client_id, diff))
            });
        self.mqtt_sender = mqtt.get_sender().sender_clone();
    }

    pub fn connect_fs(&mut self, fs: &mut impl MessageSource<FsWatchEvent, PathBuf>) {
        fs.connect_mapped_sink(
            self.processor.registry.config_dir().into(),
            &self.message_box,
            |msg| Some(InputMessage::FsWatchEvent(msg)),
        );
    }

    fn topics(&self) -> TopicFilter {
        self.processor.subscriptions()
    }
}

impl MessageSource<WatchRequest, NoConfig> for FlowsMapperBuilder {
    fn connect_sink(&mut self, _config: NoConfig, peer: &impl MessageSink<WatchRequest>) {
        self.watch_request_sender = peer.get_sender();
    }
}

impl MessageSink<WatchEvent> for FlowsMapperBuilder {
    fn get_sender(&self) -> DynSender<WatchEvent> {
        self.message_box.get_sender().sender_clone()
    }
}

impl RuntimeRequestSink for FlowsMapperBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.message_box.get_signal_sender()
    }
}

impl Builder<FlowsMapper> for FlowsMapperBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<FlowsMapper, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> FlowsMapper {
        let subscriptions = self.topics().clone();
        let watched_commands = HashSet::new();
        FlowsMapper {
            messages: self.message_box.build(),
            mqtt_sender: self.mqtt_sender,
            watch_request_sender: self.watch_request_sender,
            subscriptions,
            watched_commands,
            processor: self.processor,
            next_dump: Instant::now() + STATS_DUMP_INTERVAL,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum LoadError {
    #[error("JavaScript module not found: {module_name}")]
    UnknownModule { module_name: String },

    #[error("JavaScript function not found: {function} in {module_name}")]
    UnknownFunction {
        module_name: String,
        function: String,
    },

    #[error("Cannot read file {path}: {error}")]
    ReadError {
        path: Utf8PathBuf,
        error: std::io::Error,
    },

    #[error(transparent)]
    TomlError(#[from] toml::de::Error),

    #[error(transparent)]
    JsError(#[from] rquickjs::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl LoadError {
    pub fn from_io(error: std::io::Error, path: &Utf8Path) -> Self {
        LoadError::ReadError {
            path: path.to_owned(),
            error,
        }
    }
}
