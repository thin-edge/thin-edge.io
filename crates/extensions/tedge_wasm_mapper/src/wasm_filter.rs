use crate::engine::HostState;
use std::ops::DerefMut;
use std::sync::Arc;
use std::sync::Mutex;
use time::OffsetDateTime;
use tracing::debug;
use wasmtime::component::Component;
use wasmtime::component::Linker;
use wasmtime::Store;

wasmtime::component::bindgen!({
    path: "wit/filter.wit",
    world: "tedge",
});

use crate::pipeline::Filter;
use crate::pipeline::FilterError;
use crate::LoadError;
use exports::tedge::filter::filtering as wasm;
use tedge_mqtt_ext::MqttMessage;

pub struct WasmFilterResource {
    store: Arc<Mutex<Store<HostState>>>,
    component: Tedge,
    filter: wasm::Filter,
}

impl WasmFilterResource {
    pub fn try_new(
        mut store: Store<HostState>,
        component: &Component,
        linker: &Linker<HostState>,
        config: &MqttMessage,
    ) -> Result<Self, LoadError> {
        let component = Tedge::instantiate(&mut store, component, linker)?;
        let config = wasm::Message::try_from(config).unwrap(); // FIXME
        let filter = component
            .tedge_filter_filtering()
            .filter()
            .call_constructor(&mut store, &config)?;
        let store = Arc::new(Mutex::new(store));
        Ok(WasmFilterResource {
            store,
            component,
            filter,
        })
    }

    pub fn guest(&self) -> &wasm::Guest {
        self.component.tedge_filter_filtering()
    }

    pub fn into_dyn(self) -> Box<dyn Filter> {
        Box::new(self)
    }
}

impl Filter for WasmFilterResource {
    fn process(
        &mut self,
        timestamp: OffsetDateTime,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, FilterError> {
        debug!(target: "WASM", "process({timestamp}, {message:?})");
        let timestamp = timestamp.try_into()?;
        let message = wasm::Message::try_from(message)?;
        let mut store = self.store.lock().unwrap();
        let result = self
            .guest()
            .filter()
            .call_process(store.deref_mut(), self.filter, timestamp, &message)
            .map_err(|err| {
                FilterError::IncorrectSetting(format!("failed to call the process method: {}", err))
            })?;
        result
            .map_err(FilterError::from)?
            .into_iter()
            .map(MqttMessage::try_from)
            .collect()
    }

    fn update_config(&mut self, config: &MqttMessage) -> Result<(), FilterError> {
        debug!(target: "WASM", "update_config({config:?})");
        let config = wasm::Message::try_from(config)?;
        let mut store = self.store.lock().unwrap();
        let result = self
            .guest()
            .filter()
            .call_update_config(store.deref_mut(), self.filter, &config)
            .map_err(|err| {
                FilterError::IncorrectSetting(format!("failed to call the config method: {}", err))
            })?;
        result.map_err(FilterError::from)
    }

    fn tick(&mut self, timestamp: OffsetDateTime) -> Result<Vec<MqttMessage>, FilterError> {
        debug!(target: "WASM", "tick({timestamp})");
        let timestamp = timestamp.try_into()?;
        let mut store = self.store.lock().unwrap();
        let result = self
            .guest()
            .filter()
            .call_tick(store.deref_mut(), self.filter, timestamp)
            .map_err(|err| {
                FilterError::IncorrectSetting(format!("failed to call the tick method: {}", err))
            })?;
        result
            .map_err(FilterError::from)?
            .into_iter()
            .map(MqttMessage::try_from)
            .collect()
    }
}

impl TryFrom<OffsetDateTime> for wasm::Datetime {
    type Error = FilterError;

    fn try_from(value: OffsetDateTime) -> Result<Self, Self::Error> {
        let seconds = u64::try_from(value.unix_timestamp()).map_err(|err| {
            FilterError::UnsupportedMessage(format!("failed to convert timestamp: {}", err))
        })?;

        Ok(wasm::Datetime {
            seconds,
            nanoseconds: value.nanosecond(),
        })
    }
}

impl TryFrom<&MqttMessage> for wasm::Message {
    type Error = FilterError;

    fn try_from(message: &MqttMessage) -> Result<Self, Self::Error> {
        let topic = message.topic.to_string();
        let payload = message
            .payload_str()
            .map_err(|_| FilterError::UnsupportedMessage("Not an UTF8 payload".to_string()))?
            .to_string();
        Ok(wasm::Message { topic, payload })
    }
}

impl TryFrom<wasm::Message> for MqttMessage {
    type Error = FilterError;

    fn try_from(message: wasm::Message) -> Result<Self, Self::Error> {
        let topic = message.topic.as_str().try_into().map_err(|_| {
            FilterError::UnsupportedMessage(format!("invalid topic {}", message.topic))
        })?;
        Ok(MqttMessage::new(&topic, message.payload))
    }
}

impl From<wasm::FilterError> for FilterError {
    fn from(error: wasm::FilterError) -> Self {
        match error {
            wasm::FilterError::UnsupportedMessage(err) => FilterError::UnsupportedMessage(err),
            wasm::FilterError::IncorrectSetting(err) => FilterError::IncorrectSetting(err),
        }
    }
}
