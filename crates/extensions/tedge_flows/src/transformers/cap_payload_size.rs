use crate::config::ConfigError;
use crate::js_value::JsonValue;
use crate::transformers::Transformer;
use crate::FlowError;
use crate::Message;
use std::time::SystemTime;

#[derive(Clone, Default)]
pub struct CapPayloadSize {
    max_size: Option<usize>,
    discard: bool,
}

impl Transformer for CapPayloadSize {
    fn name(&self) -> &str {
        "cap-payload-size"
    }

    fn set_config(&mut self, config: JsonValue) -> Result<(), ConfigError> {
        self.max_size = config
            .number_property("max_size")
            .and_then(|n| n.as_u64())
            .map(|n| n as usize);
        self.discard = config.bool_property("discard").unwrap_or(false);
        Ok(())
    }

    fn on_message(
        &self,
        _timestamp: SystemTime,
        message: &Message,
    ) -> Result<Vec<Message>, FlowError> {
        if let Some(max_size) = self.max_size {
            if message.payload.len() > max_size {
                if self.discard {
                    return Ok(vec![]);
                } else {
                    return Err(FlowError::UnsupportedMessage(format!(
                        "Payload is too large >{max_size}"
                    )));
                }
            }
        }
        Ok(vec![message.clone()])
    }
}
