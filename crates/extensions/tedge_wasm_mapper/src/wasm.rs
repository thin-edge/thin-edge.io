use crate::engine::HostState;
use crate::pipeline;
use crate::pipeline::Filter;
use std::ops::DerefMut;
use std::sync::Arc;
use std::sync::Mutex;
use tedge_mqtt_ext::MqttMessage;
use time::OffsetDateTime;
use tracing::debug;
use wasmtime::component::TypedFunc;
use wasmtime::Store;

wasmtime::component::bindgen!({
    path: "wit/world.wit",
    world: "tedge",
});

pub type TransformedMessages = Result<Vec<Message>, FilterError>;
pub type ProcessFunc = TypedFunc<(Datetime, Message), (TransformedMessages,)>;

pub struct WasmFilter {
    store: Arc<Mutex<Store<HostState>>>,
    process_func: ProcessFunc,
}

impl WasmFilter {
    pub fn new(store: Store<HostState>, process_func: ProcessFunc) -> Self {
        let store = Arc::new(Mutex::new(store));

        Self {
            store,
            process_func,
        }
    }

    pub fn into_dyn(self) -> Box<dyn Filter> {
        Box::new(self)
    }
}

impl Filter for WasmFilter {
    fn process(
        &mut self,
        timestamp: OffsetDateTime,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, pipeline::FilterError> {
        debug!(target: "WASM", "process({timestamp}, {message:?})");
        let timestamp = timestamp.try_into()?;
        let message = message.try_into()?;

        let mut store = self.store.lock().unwrap();
        let (result,) = self
            .process_func
            .call(store.deref_mut(), (timestamp, message))
            .map_err(|err| {
                pipeline::FilterError::IncorrectSetting(format!(
                    "failed to call the process function: {}",
                    err
                ))
            })?;
        self.process_func
            .post_return(store.deref_mut())
            .map_err(|err| {
                pipeline::FilterError::IncorrectSetting(format!(
                    "failed to clean up the process function call: {}",
                    err
                ))
            })?;

        result
            .map_err(pipeline::FilterError::from)?
            .into_iter()
            .map(MqttMessage::try_from)
            .collect()
    }

    fn update_config(&mut self, config: &MqttMessage) -> Result<(), pipeline::FilterError> {
        debug!(target: "WASM", "update_config({config:?})");
        Ok(())
    }

    fn tick(
        &mut self,
        timestamp: OffsetDateTime,
    ) -> Result<Vec<MqttMessage>, pipeline::FilterError> {
        debug!(target: "WASM", "tick({timestamp})");
        Ok(vec![])
    }
}

impl TryFrom<OffsetDateTime> for Datetime {
    type Error = pipeline::FilterError;

    fn try_from(value: OffsetDateTime) -> Result<Self, Self::Error> {
        let seconds = u64::try_from(value.unix_timestamp()).map_err(|err| {
            pipeline::FilterError::UnsupportedMessage(format!(
                "failed to convert timestamp: {}",
                err
            ))
        })?;

        Ok(Datetime {
            seconds,
            nanoseconds: value.nanosecond(),
        })
    }
}

impl TryFrom<&MqttMessage> for Message {
    type Error = pipeline::FilterError;

    fn try_from(message: &MqttMessage) -> Result<Self, Self::Error> {
        let topic = message.topic.to_string();
        let payload = message
            .payload_str()
            .map_err(|_| {
                pipeline::FilterError::UnsupportedMessage("Not an UTF8 payload".to_string())
            })?
            .to_string();
        Ok(Message { topic, payload })
    }
}

impl TryFrom<Message> for MqttMessage {
    type Error = pipeline::FilterError;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        let topic = message.topic.as_str().try_into().map_err(|_| {
            pipeline::FilterError::UnsupportedMessage(format!("invalid topic {}", message.topic))
        })?;
        Ok(MqttMessage::new(&topic, message.payload))
    }
}

impl From<FilterError> for pipeline::FilterError {
    fn from(error: FilterError) -> Self {
        match error {
            FilterError::UnsupportedMessage(err) => pipeline::FilterError::UnsupportedMessage(err),
            FilterError::IncorrectSetting(err) => pipeline::FilterError::IncorrectSetting(err),
        }
    }
}
