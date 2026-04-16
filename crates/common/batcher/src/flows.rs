use crate::batcher::BatcherOutput;
use crate::Batchable;
use crate::Batcher;
use serde_json::json;
use serde_json::Value;
use std::time::SystemTime;
use tedge_flows::ConfigError;
use tedge_flows::FlowContextHandle;
use tedge_flows::FlowError;
use tedge_flows::JsonValue;
use tedge_flows::Message;
use tedge_flows::Transformer;
use time::OffsetDateTime;

#[derive(Default)]
pub struct MessageBatcher {
    batcher: Batcher<Message>,
    batch_topic: String,
}

impl Batchable for Message {
    type Key = String;

    fn key(&self) -> Self::Key {
        self.topic.clone()
    }

    fn event_time(&self) -> OffsetDateTime {
        self.timestamp
            .map(|t| t.into())
            .unwrap_or_else(OffsetDateTime::now_utc)
    }
}

impl Clone for MessageBatcher {
    fn clone(&self) -> MessageBatcher {
        MessageBatcher::default()
    }
}

impl Transformer for MessageBatcher {
    fn name(&self) -> &str {
        "time-window-batcher"
    }

    fn set_config(&mut self, config: JsonValue) -> Result<(), ConfigError> {
        // TODO: We should expose : event_jitter, delivery_jitter and message_leap_limit
        if let Some(topic) = config.string_property("topic") {
            self.batch_topic = topic.to_owned();
        }
        Ok(())
    }

    fn on_message(
        &mut self,
        timestamp: SystemTime,
        message: &Message,
        _context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        let batches = self
            .batcher
            .event(timestamp.into(), message.clone())
            .into_iter()
            .filter_map(|action| match action {
                BatcherOutput::Batch(batch) => Some(batch),
                BatcherOutput::Timer(_) => None,
            });
        self.batch_message_batches(batches)
    }

    fn is_periodic(&self) -> bool {
        true
    }

    fn on_interval(
        &mut self,
        timestamp: SystemTime,
        _context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        let batches = self.batcher.time(timestamp.into());
        self.batch_message_batches(batches)
    }
}

impl MessageBatcher {
    /// Build a message from a batch of messages
    ///
    /// Assume each message payload can be translated to JSON
    /// Build a message which payload is a JSON array of all the messages
    fn batch_messages(&self, messages: Vec<Message>) -> Result<Message, FlowError> {
        let mut batch = vec![];

        for message in messages {
            let Some(utf8_payload) = message.payload_str() else {
                return Err(FlowError::UnsupportedMessage(
                    "Cannot batch non UTF-8 message".to_owned(),
                ));
            };
            let payload: Value = match serde_json::from_str(utf8_payload) {
                Ok(payload) => payload,
                Err(_) => json!(utf8_payload),
            };
            batch.push(json!({
                "topic": message.topic,
                "payload": payload,
            }))
        }

        Ok(Message::new(
            self.batch_topic.clone(),
            Value::Array(batch).to_string(),
        ))
    }

    fn batch_message_batches(
        &self,
        batches: impl IntoIterator<Item = Vec<Message>>,
    ) -> Result<Vec<Message>, FlowError> {
        batches
            .into_iter()
            .map(|batch| self.batch_messages(batch))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn single_event_batch() {
        let context = FlowContextHandle::default();
        let mut batcher = MessageBatcher::default();
        batcher
            .set_config(json!({"topic": "batch"}).into())
            .unwrap();

        let now = SystemTime::now();
        let msg = Message::with_timestamp("a/b", "42", now);
        assert!(batcher.on_message(now, &msg, &context).unwrap().is_empty());

        let later = now + Duration::from_secs(5);
        let batch = batcher.on_interval(later, &context).unwrap();
        assert_batch_eq(
            batch,
            "batch",
            json!([
               {"topic": "a/b", "payload": 42},
            ]),
        );
    }

    #[test]
    fn multi_event_batch() {
        let context = FlowContextHandle::default();
        let mut batcher = MessageBatcher::default();
        batcher
            .set_config(json!({"topic": "batch"}).into())
            .unwrap();

        let now = SystemTime::now();
        let msg = Message::with_timestamp("payload/num", "42", now);
        assert!(batcher.on_message(now, &msg, &context).unwrap().is_empty());

        let later = now + Duration::from_millis(5);
        let msg = Message::with_timestamp("payload/string", r#"124|456.789"#, now);
        assert!(batcher
            .on_message(later, &msg, &context)
            .unwrap()
            .is_empty());

        let later = now + Duration::from_millis(10);
        let msg = Message::with_timestamp("payload/json", r#"{"foo": "bar"}"#, now);
        assert!(batcher
            .on_message(later, &msg, &context)
            .unwrap()
            .is_empty());

        let later = now + Duration::from_secs(5);
        let batch = batcher.on_interval(later, &context).unwrap();
        assert_batch_eq(
            batch,
            "batch",
            json!([
               {"topic": "payload/num", "payload": 42},
               {"topic": "payload/string", "payload": "124|456.789"},
               {"topic": "payload/json", "payload": {"foo": "bar"}},
            ]),
        );
    }

    fn assert_batch_eq(batch: Vec<Message>, topic: &str, mut expected_payload: Value) {
        expected_payload
            .as_array_mut()
            .unwrap()
            .sort_by_key(|msg| msg.get("topic").unwrap().to_string());

        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].topic, topic);
        let mut actual_payload: Value =
            serde_json::from_slice(batch[0].payload.as_slice()).unwrap();
        actual_payload
            .as_array_mut()
            .unwrap()
            .sort_by_key(|msg| msg.get("topic").unwrap().to_string());

        assert_eq!(actual_payload, expected_payload);
    }
}
