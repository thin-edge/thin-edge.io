use serde_json::Value;
use tedge_utils::timestamp::IsoOrUnix;
use tedge_utils::timestamp::TimeFormat;
use time::OffsetDateTime;

use crate::ConfigError;
use crate::FlowContextHandle;
use crate::FlowError;
use crate::JsonValue;
use crate::Message;
use crate::Transformer;
use std::collections::HashMap;
use std::time::Duration;
use std::time::SystemTime;

/// Group `te` measurements observed during a time window.
///
/// Measurements are batched using event time, but a batch can be closed for two independent reasons:
///
/// - A measurement has been received with an event time that is more recent than the maximum accepted by this batch
///   This logic is implemented by the `Batch::try_merge` method.
/// - More time than the batch time window has elapsed since the batch has been opened,
///   meaning no more measurement for the same batch can be possibly received
///   (assuming both time are roughly moving the same speed).
///   This logic is implemented by `Batch::can_be_closed`.
///
/// Actually should work on any kind of messages as long as the payloads are JSON objects
/// with an event `time` property encoded either as `unix` or `rfc3339`.
///
/// - any message that cannot be processed (e.g.not a JSON object) is forwarded unchanged
/// - measurements published on different topics are never grouped together
/// - measurements with conflicting values are never grouped together
#[derive(Clone)]
pub struct GroupMeasurements {
    time_window: Duration,
    batches: HashMap<String, Batch>,
}

#[derive(Clone)]
struct Batch {
    /// System time of when this batch has been started
    ///
    /// This is used to close the batch on interval
    system_time: SystemTime,

    /// Event time of the oldest message
    from_event_time: OffsetDateTime,

    /// Event time of the most recent message
    to_event_time: OffsetDateTime,

    /// Json object grouping the payloads of that batch
    payload: serde_json::Map<String, serde_json::Value>,
}

impl Default for GroupMeasurements {
    fn default() -> Self {
        Self {
            time_window: Duration::from_millis(500),
            batches: HashMap::new(),
        }
    }
}

impl Transformer for GroupMeasurements {
    fn name(&self) -> &str {
        "group-measurements"
    }

    fn set_config(&mut self, config: JsonValue) -> Result<(), ConfigError> {
        if let Some(time_window) = config.string_property("time_window") {
            let Ok(duration) = humantime::parse_duration(time_window) else {
                return Err(ConfigError::IncorrectSetting(format!(
                    "Invalid time_window: not a duration: {time_window}"
                )));
            };
            self.time_window = duration;
        };

        Ok(())
    }

    fn on_message(
        &mut self,
        timestamp: SystemTime,
        message: &Message,
        _context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        let Some(batch_seed) = BatchSeed::new("time", timestamp, message) else {
            return Ok(vec![message.clone()]);
        };

        let topic = message.topic.clone();
        match self.batches.remove(&topic) {
            None => {
                self.batches.insert(topic, batch_seed.into_batch());
                Ok(vec![])
            }

            Some(previous_batch) => {
                let (updated_batch, maybe_closed) =
                    previous_batch.try_merge(batch_seed, self.time_window);
                self.batches.insert(topic.clone(), updated_batch);
                match maybe_closed {
                    Some(closed_batch) => Ok(vec![closed_batch.into_message(topic)]),
                    None => Ok(vec![]),
                }
            }
        }
    }

    fn is_periodic(&self) -> bool {
        true
    }

    fn on_interval(
        &mut self,
        timestamp: std::time::SystemTime,
        _context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        // Using `self.batches.extract_if` would be more appropriate but requires rust 1.88
        let mut closed_batches = vec![];
        self.batches.retain(|topic, batch| {
            let can_be_closed = batch.can_be_closed(timestamp, self.time_window);
            if can_be_closed {
                closed_batches.push(batch.clone().into_message(topic.clone()))
            }
            !can_be_closed
        });
        Ok(closed_batches)
    }
}

impl Batch {
    /// Use system time to close a batch for which no more message could be received.
    fn can_be_closed(&self, timestamp: SystemTime, time_window: Duration) -> bool {
        self.system_time + time_window < timestamp
    }

    fn into_message(mut self, topic: String) -> Message {
        if let Ok(time) = TimeFormat::Unix.to_json(self.from_event_time) {
            self.payload.insert("time".to_string(), time);
        }
        let payload = serde_json::Value::Object(self.payload).to_string();
        Message::new(topic, payload)
    }

    /// Try to merge a batch seed into this batch.
    ///
    /// Return the updated batch, and possibly an older batch to be closed.
    fn try_merge(mut self, candidate: BatchSeed, time_window: Duration) -> (Batch, Option<Batch>) {
        if candidate.event_time >= self.from_event_time + time_window {
            // the current batch is too old to include the new item and can be replaced by the new candidate
            return (candidate.into_batch(), Some(self));
        }

        if candidate.event_time + time_window <= self.to_event_time {
            // the batch candidate is arrived so late, that it can no more be grouped
            return (self, Some(candidate.into_batch()));
        }

        if some_conflicting_values(&self.payload, &candidate.payload) {
            // never erase a value, hence replacing the current batch with the new candidate
            return (candidate.into_batch(), Some(self));
        }

        // The candidate can be merged in the current batch
        merge_values(&mut self.payload, candidate.payload);
        if candidate.event_time < self.from_event_time {
            self.from_event_time = candidate.event_time;
        }
        if self.to_event_time < candidate.event_time {
            self.to_event_time = candidate.event_time;
        }
        assert!(self.from_event_time <= self.to_event_time);
        (self, None)
    }
}

/// A single message wrapped as a batch candidate
struct BatchSeed {
    system_time: SystemTime,
    event_time: OffsetDateTime,
    payload: serde_json::Map<String, serde_json::Value>,
}

impl BatchSeed {
    fn new(time_property: &str, system_time: SystemTime, message: &Message) -> Option<BatchSeed> {
        let Ok(serde_json::Value::Object(mut payload)) =
            serde_json::from_slice(message.payload.as_slice())
        else {
            return None;
        };

        let time_value = payload.remove(time_property)?;
        let event_time = IsoOrUnix::try_from(&time_value)
            .map(|t| t.into_inner())
            .ok()?;

        Some(BatchSeed {
            system_time,
            event_time,
            payload,
        })
    }

    fn into_batch(self) -> Batch {
        Batch {
            system_time: self.system_time,
            from_event_time: self.event_time,
            to_event_time: self.event_time,
            payload: self.payload,
        }
    }
}

fn some_conflicting_values(
    left: &serde_json::Map<String, serde_json::Value>,
    right: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    for (key, right_value) in right {
        match (left.get(key), right_value) {
            (Some(Value::Object(left_inner_values)), Value::Object(right_inner_values)) => {
                if some_conflicting_values(left_inner_values, right_inner_values) {
                    return true;
                }
            }
            (Some(left_value), right_value) => {
                if left_value != right_value {
                    return true;
                }
            }
            (None, _) => {}
        }
    }
    false
}

fn merge_values(
    left: &mut serde_json::Map<String, serde_json::Value>,
    right: serde_json::Map<String, serde_json::Value>,
) {
    for (key, right_value) in right {
        match (left.get_mut(&key), right_value) {
            (Some(Value::Object(left_inner_values)), Value::Object(right_inner_values)) => {
                merge_values(left_inner_values, right_inner_values);
            }
            (Some(left_value), right_value) => {
                assert_eq!(left_value, &right_value, "Merging conflicting values")
            }
            (None, right_value) => {
                left.insert(key, right_value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn measurements_received_during_the_configured_time_window_are_grouped() {
        let context = FlowContextHandle::default();
        let mut batcher = GroupMeasurements::default(); // time window = 500 ms

        let mut system_time = SystemTime::now();
        let event_time = 1779887987.138;

        // All messages received during the time window are batched
        for (topic, payload) in [
            ("topic/1", json!({ "time": event_time, "a": 1.0 })),
            ("topic/1", json!({ "time": event_time + 0.045, "b": 2.0 })),
            ("topic/1", json!({ "time": event_time + 0.078, "c": 3.0 })),
        ] {
            let msg = Message::new(topic, payload.to_string());
            system_time += millis(1);
            assert!(batcher
                .on_message(system_time, &msg, &context)
                .unwrap()
                .is_empty());
        }

        // When a message is received with an event time exceeding the time window, the batch is published
        let msg4 = Message::new(
            "topic/1",
            json!({ "time": event_time + 0.650, "d": 4.0 }).to_string(),
        );
        let batch = batcher.on_message(system_time, &msg4, &context).unwrap();
        assert_eq!(batch.len(), 1);
        let msg = batch.get(0).unwrap();
        assert_eq!(msg.topic, "topic/1");
        assert_eq!(
            msg.payload_str().unwrap(),
            r#"{"a":1.0,"b":2.0,"c":3.0,"time":1779887987.1379998}"#
        );
    }

    #[test]
    fn measurements_received_during_the_configured_time_window_are_grouped_even_if_out_of_order() {
        let context = FlowContextHandle::default();
        let mut batcher = GroupMeasurements::default();

        let mut system_time = SystemTime::now();
        let event_time = 1779887987.138;

        // The messages are batched as long as their event timestamp fit the time window,
        for (topic, payload) in [
            ("topic/1", json!({ "time": event_time, "a": 1.0 })),
            ("topic/1", json!({ "time": event_time - 0.045, "b": 2.0 })),
            ("topic/1", json!({ "time": event_time - 0.078, "c": 3.0 })),
        ] {
            let msg = Message::new(topic, payload.to_string());
            system_time += millis(1);
            assert!(batcher
                .on_message(system_time, &msg, &context)
                .unwrap()
                .is_empty());
        }

        // When a message is received with an event time exceeding the time window, the batch is published
        let msg4 = Message::new(
            "topic/1",
            json!({ "time": event_time + 0.650, "d": 4.0 }).to_string(),
        );
        let batch = batcher.on_message(system_time, &msg4, &context).unwrap();
        assert_eq!(batch.len(), 1);
        let msg = batch.get(0).unwrap();
        assert_eq!(msg.topic, "topic/1");

        // The event-time assigned to the batch is the oldest one
        assert_eq!(
            msg.payload_str().unwrap(),
            r#"{"a":1.0,"b":2.0,"c":3.0,"time":1779887987.06}"#
        );
    }

    #[test]
    fn when_no_more_measurements_is_received_during_the_configured_time_window_the_batches_are_published_on_interval(
    ) {
        let context = FlowContextHandle::default();
        let mut batcher = GroupMeasurements::default(); // time window = 500 ms

        let mut system_time = SystemTime::now();
        let event_time = 1779887987.138;

        // The batcher keeps batching messages when they arrive
        for (topic, payload) in [
            ("topic/1", json!({ "time": event_time, "a": 1.0 })),
            ("topic/1", json!({ "time": event_time + 0.045, "b": 2.0 })),
            ("topic/1", json!({ "time": event_time + 0.078, "c": 3.0 })),
        ] {
            let msg = Message::new(topic, payload.to_string());
            system_time += millis(1);
            assert!(batcher
                .on_message(system_time, &msg, &context)
                .unwrap()
                .is_empty());
        }

        // However when no more message is received, the batcher has to release the current batch
        // On interval, a check is done that no more message are to be expected for a batch

        // if on_interval is called too soon, no messages is released
        assert!(batcher
            .on_interval(system_time + millis(50), &context)
            .unwrap()
            .is_empty());

        // Till not released, the batch keeps growing
        let (topic, payload) = ("topic/1", json!({ "time": event_time + 0.19, "d": 4.0 }));
        let msg = Message::new(topic, payload.to_string());
        assert!(batcher
            .on_message(system_time + millis(51), &msg, &context)
            .unwrap()
            .is_empty());

        // When the time window has elapsed, the batch is released
        let batch = batcher
            .on_interval(system_time + millis(600), &context)
            .unwrap();
        assert_eq!(batch.len(), 1);
        let msg = batch.get(0).unwrap();
        assert_eq!(msg.topic, "topic/1");
        assert_eq!(
            msg.payload_str().unwrap(),
            r#"{"a":1.0,"b":2.0,"c":3.0,"d":4.0,"time":1779887987.1379998}"#
        );
    }

    #[test]
    fn messages_received_on_different_topics_are_batched_independently() {
        let context = FlowContextHandle::default();
        let mut batcher = GroupMeasurements::default(); // time window = 500 ms

        let mut system_time = SystemTime::now();
        let event_time = 1779887987.0;

        for (topic, payload) in [
            ("topic/1", json!({ "time": event_time, "a": 1.0 })),
            ("topic/2", json!({ "time": event_time - 0.1, "x": 1.0 })),
            ("topic/1", json!({ "time": event_time + 0.12, "b": 2.0 })),
            ("topic/2", json!({ "time": event_time + 0.2, "y": 2.0 })),
            ("topic/2", json!({ "time": event_time + 0.3, "z": 3.0 })),
            ("topic/1", json!({ "time": event_time + 0.24, "c": 3.0 })),
        ] {
            let msg = Message::new(topic, payload.to_string());
            system_time += millis(1);
            assert!(batcher
                .on_message(system_time, &msg, &context)
                .unwrap()
                .is_empty());
        }

        let batch = batcher
            .on_interval(system_time + millis(600), &context)
            .unwrap();
        let messages = extract_topic_payload(2, batch);
        for (topic, payload) in [
            (
                "topic/1",
                json!({ "time": event_time, "a": 1.0, "b": 2.0, "c": 3.0}),
            ),
            (
                "topic/2",
                json!({ "time": event_time - 0.1, "x": 1.0, "y": 2.0, "z": 3.0}),
            ),
        ] {
            assert_eq!(messages.get(topic).unwrap(), &payload.to_string())
        }
    }

    #[test]
    fn messages_with_conflicting_values_are_batched_independently() {
        let context = FlowContextHandle::default();
        let mut batcher = GroupMeasurements::default(); // time window = 500 ms

        let mut system_time = SystemTime::now();
        let event_time = 1779887987.0;

        // Start batching messages
        for (topic, payload) in [
            ("topic/1", json!({ "time": event_time, "a": 1.0 })),
            ("topic/1", json!({ "time": event_time + 0.12, "b": 2.0 })),
            ("topic/1", json!({ "time": event_time + 0.24, "c": 3.0 })),
        ] {
            let msg = Message::new(topic, payload.to_string());
            system_time += millis(1);
            assert!(batcher
                .on_message(system_time, &msg, &context)
                .unwrap()
                .is_empty());
        }

        // The batch is closed when a message is received with a conflicting value - despite the time window is not elapsed
        let conflicting_msg = Message::new(
            "topic/1",
            json!({ "time": event_time + 0.26, "a": 4.0 }).to_string(),
        );
        let batch = batcher
            .on_message(system_time + millis(1), &conflicting_msg, &context)
            .unwrap();
        let messages = extract_topic_payload(1, batch);
        for (topic, payload) in [(
            "topic/1",
            json!({ "time": event_time, "a": 1.0, "b": 2.0, "c": 3.0}),
        )] {
            assert_eq!(messages.get(topic).unwrap(), &payload.to_string())
        }

        // The message with a conflicting value is used to start a new batch
        let batch = batcher
            .on_interval(system_time + millis(600), &context)
            .unwrap();
        let messages = extract_topic_payload(1, batch);
        for (topic, payload) in [("topic/1", json!({ "time": event_time + 0.26, "a": 4.0}))] {
            assert_eq!(messages.get(topic).unwrap(), &payload.to_string())
        }
    }

    #[test]
    fn messages_that_cannot_be_processed_are_forwarded() {
        let context = FlowContextHandle::default();
        let mut batcher = GroupMeasurements::default(); // time window = 500 ms

        let system_time = SystemTime::now();
        let msg = Message::new("te/d/main//m/", "not a valid thin-edge measurement");

        let batch = batcher.on_message(system_time, &msg, &context).unwrap();
        assert_eq!(batch.len(), 1);
        let msg = batch.get(0).unwrap();
        assert_eq!(
            msg.payload_str().unwrap(),
            "not a valid thin-edge measurement"
        );
    }

    fn millis(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn extract_topic_payload(
        expected_count: usize,
        batch: Vec<Message>,
    ) -> HashMap<String, String> {
        assert_eq!(batch.len(), expected_count);
        batch
            .into_iter()
            .map(|msg| (msg.topic, String::from_utf8(msg.payload).unwrap()))
            .collect()
    }
}
