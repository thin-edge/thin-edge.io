use crate::js_value::JsonValue;
use crate::transformers::Transformer;
use crate::FlowError;
use crate::Message;
use std::time::SystemTime;

#[derive(Clone, Default)]
pub struct CapPayloadSize;

impl Transformer for CapPayloadSize {
    fn name(&self) -> &str {
        "cap-payload-size"
    }

    fn on_message(
        &self,
        _timestamp: SystemTime,
        message: &Message,
        config: &JsonValue,
    ) -> Result<Vec<Message>, FlowError> {
        if let Some(max_size) = config.number_property("max_size").and_then(|n| n.as_u64()) {
            if message.payload.len() > max_size as usize {
                if config.bool_property("discard").unwrap_or(false) {
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
