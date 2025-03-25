use crate::pipeline::Filter;
use crate::pipeline::FilterError;
use std::path::PathBuf;
use tedge_mqtt_ext::MqttMessage;
use time::OffsetDateTime;
use tracing::debug;

pub struct WasmFilter {}

impl WasmFilter {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        debug!(target: "WASM", "new({path:?})");
        WasmFilter {}
    }
}

impl Filter for WasmFilter {
    fn process(
        &mut self,
        timestamp: OffsetDateTime,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, FilterError> {
        debug!(target: "WASM", "process({timestamp}, {message:?})");
        Ok(vec![message.clone()])
    }

    fn update_config(&mut self, config: &MqttMessage) -> Result<(), FilterError> {
        debug!(target: "WASM", "update_config({config:?})");
        Ok(())
    }

    fn tick(&mut self, timestamp: OffsetDateTime) -> Result<Vec<MqttMessage>, FilterError> {
        debug!(target: "WASM", "tick({timestamp})");
        Ok(vec![])
    }
}
