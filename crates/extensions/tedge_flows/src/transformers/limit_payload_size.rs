use crate::config::ConfigError;
use crate::js_value::JsonValue;
use crate::transformers::Transformer;
use crate::FlowContextHandle;
use crate::FlowError;
use crate::Message;
use std::time::SystemTime;

#[derive(Clone, Default)]
pub struct LimitPayloadSize {
    max_size: Option<usize>,
    discard: bool,
}

impl Transformer for LimitPayloadSize {
    fn name(&self) -> &str {
        "limit-payload-size"
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
        &mut self,
        _timestamp: SystemTime,
        message: &Message,
        _context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        if let Some(max_size) = self.max_size {
            if message.wire_size() > max_size {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_message_within_packet_limit() {
        let message = Message::new("topic", "payload");
        let mut transformer = transformer(message.wire_size(), false);

        assert_eq!(forward(&mut transformer, &message).unwrap(), vec![message]);
    }

    #[test]
    fn rejects_message_whose_packet_exceeds_limit_even_if_body_fits() {
        // The body alone fits the limit, but the full packet does not.
        let message = Message::new("some/topic", vec![b'x'; 10]);
        let max_size = message.payload.len() + 2;
        assert!(message.payload.len() <= max_size);
        assert!(message.wire_size() > max_size);

        let mut transformer = transformer(max_size, false);
        assert!(matches!(
            forward(&mut transformer, &message),
            Err(FlowError::UnsupportedMessage(_))
        ));
    }

    #[test]
    fn discards_over_limit_message_when_configured() {
        let message = Message::new("some/topic", vec![b'x'; 10]);
        let max_size = message.payload.len() + 2;

        let mut transformer = transformer(max_size, true);
        assert_eq!(forward(&mut transformer, &message).unwrap(), vec![]);
    }

    fn transformer(max_size: usize, discard: bool) -> LimitPayloadSize {
        LimitPayloadSize {
            max_size: Some(max_size),
            discard,
        }
    }

    fn forward(
        transformer: &mut LimitPayloadSize,
        message: &Message,
    ) -> Result<Vec<Message>, FlowError> {
        transformer.on_message(
            SystemTime::UNIX_EPOCH,
            message,
            &FlowContextHandle::default(),
        )
    }
}
