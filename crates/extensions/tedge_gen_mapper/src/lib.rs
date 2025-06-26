mod actor;
mod config;
mod js_filter;
mod js_runtime;
pub mod pipeline;
mod runtime;

use crate::actor::GenMapper;
pub use crate::runtime::MessageProcessor;
use std::convert::Infallible;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tedge_actors::fan_in_message_type;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::DynSubscriptions;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::MqttRequest;
use tedge_mqtt_ext::SubscriptionDiff;
use tedge_mqtt_ext::TopicFilter;
use tracing::error;

fan_in_message_type!(InputMessage[MqttMessage, FsWatchEvent]: Clone, Debug, Eq, PartialEq);
fan_in_message_type!(OutputMessage[MqttMessage, SubscriptionDiff]: Clone, Debug, Eq, PartialEq);

pub struct GenMapperBuilder {
    message_box: SimpleMessageBoxBuilder<InputMessage, OutputMessage>,
    subscriptions: Arc<Mutex<TopicFilter>>,
    processor: MessageProcessor,
}

impl GenMapperBuilder {
    pub async fn try_new(config_dir: impl AsRef<Path>) -> Result<Self, LoadError> {
        let processor = MessageProcessor::try_new(config_dir).await?;
        Ok(GenMapperBuilder {
            message_box: SimpleMessageBoxBuilder::new("GenMapper", 16),
            subscriptions: Arc::new(Mutex::new(TopicFilter::empty())),
            processor,
        })
    }

    pub fn connect(
        &mut self,
        mqtt: &mut (impl for<'a> MessageSource<MqttMessage, &'a mut DynSubscriptions>
                  + MessageSink<MqttRequest>),
    ) {
        let mut dyn_subscriptions = DynSubscriptions::new(self.topics());
        mqtt.connect_mapped_sink(&mut dyn_subscriptions, &self.message_box, |msg| {
            Some(InputMessage::MqttMessage(msg))
        });
        let client_id = dyn_subscriptions.client_id();
        self.message_box
            .connect_mapped_sink(NoConfig, mqtt, move |msg| match msg {
                OutputMessage::MqttMessage(mqtt) => Some(MqttRequest::Publish(mqtt)),
                OutputMessage::SubscriptionDiff(diff) => {
                    Some(MqttRequest::subscribe(client_id, diff))
                }
            });
    }

    pub fn connect_fs(&mut self, fs: &mut impl MessageSource<FsWatchEvent, PathBuf>) {
        fs.connect_mapped_sink(
            self.processor.config_dir.clone(),
            &self.message_box,
            |msg| Some(InputMessage::FsWatchEvent(msg)),
        );
    }

    fn topics(&self) -> TopicFilter {
        self.processor.subscriptions()
    }
}

impl RuntimeRequestSink for GenMapperBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.message_box.get_signal_sender()
    }
}

impl Builder<GenMapper> for GenMapperBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<GenMapper, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> GenMapper {
        GenMapper {
            messages: self.message_box.build(),
            subscriptions: self.subscriptions,
            processor: self.processor,
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

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    TomlError(#[from] toml::de::Error),

    #[error(transparent)]
    JsError(#[from] rquickjs::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}
