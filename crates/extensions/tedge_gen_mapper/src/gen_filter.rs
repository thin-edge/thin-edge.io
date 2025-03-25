use crate::pipeline::Filter;
use crate::pipeline::FilterError;
use std::path::PathBuf;
use tedge_mqtt_ext::MqttMessage;
use time::OffsetDateTime;
use tracing::debug;

/// User-defined filter
pub struct GenFilter {}

impl GenFilter {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        debug!(target: "MAPPING", "new({path:?})");
        GenFilter {}
    }
}

impl Filter for GenFilter {
    fn process(
        &mut self,
        timestamp: OffsetDateTime,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, FilterError> {
        debug!(target: "MAPPING", "process({timestamp}, {message:?})");
        Ok(vec![message.clone()])
    }

    fn update_config(&mut self, config: &MqttMessage) -> Result<(), FilterError> {
        debug!(target: "MAPPING", "update_config({config:?})");
        Ok(())
    }

    fn tick(&mut self, timestamp: OffsetDateTime) -> Result<Vec<MqttMessage>, FilterError> {
        debug!(target: "MAPPING", "tick({timestamp})");
        Ok(vec![])
    }
}
