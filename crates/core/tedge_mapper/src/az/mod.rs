use std::time::SystemTime;
use tedge_flows::ConfigError;
use tedge_flows::FlowError;
use tedge_flows::JsonValue;
use tedge_flows::Message;
use tedge_flows::Transformer;

pub mod mapper;

#[derive(Clone, Default)]
struct SkipMosquittoHealthStatus;

impl Transformer for SkipMosquittoHealthStatus {
    fn name(&self) -> &str {
        "skip-mosquitto-health-status"
    }

    fn set_config(&mut self, _config: JsonValue) -> Result<(), ConfigError> {
        Ok(())
    }

    fn on_message(
        &self,
        _timestamp: SystemTime,
        message: &Message,
    ) -> Result<Vec<Message>, FlowError> {
        // don't convert mosquitto bridge notification topic
        // https://github.com/thin-edge/thin-edge.io/issues/2236
        if let [_, _, _, _, _, "status", "health"] =
            message.topic.split('/').collect::<Vec<_>>()[..]
        {
            if message
                .payload_str()
                .map(|s| s == "0" || s == "1")
                .unwrap_or(false)
            {
                return Ok(vec![]);
            }
        }
        Ok(vec![message.clone()])
    }
}
