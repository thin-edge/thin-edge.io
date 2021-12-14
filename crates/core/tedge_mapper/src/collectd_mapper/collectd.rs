use batcher::Batchable;
use chrono::{DateTime, NaiveDateTime, Utc};
use mqtt_client::Message;
use thin_edge_json::measurement::MeasurementVisitor;

#[derive(Debug)]
pub struct CollectdMessage {
    pub metric_group_key: String,
    pub metric_key: String,
    pub timestamp: DateTime<Utc>,
    pub metric_value: f64,
}

#[derive(thiserror::Error, Debug)]
pub enum CollectdError {
    #[error(
        "Message received on invalid collectd topic: {0}. \
        Collectd message topics must be in the format collectd/<hostname>/<metric-plugin-name>/<metric-key>"
    )]
    InvalidMeasurementTopic(String),

    #[error("Invalid payload received on topic: {0}. Error: {1}")]
    InvalidMeasurementPayload(String, CollectdPayloadError),

    #[error("Non UTF-8 payload: {0:?}")]
    NonUTF8MeasurementPayload(Vec<u8>),
}

impl CollectdMessage {
    pub fn accept<T>(&self, visitor: &mut T) -> Result<(), T::Error>
    where
        T: MeasurementVisitor,
    {
        visitor.visit_grouped_measurement(
            &self.metric_group_key,
            &self.metric_key,
            self.metric_value,
        )
    }

    #[cfg(test)]
    pub fn new(
        metric_group_key: &str,
        metric_key: &str,
        metric_value: f64,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            metric_group_key: metric_group_key.to_string(),
            metric_key: metric_key.to_string(),
            timestamp,
            metric_value,
        }
    }

    pub fn parse_from(mqtt_message: &Message) -> Result<Vec<Self>, CollectdError> {
        let topic = mqtt_message.topic.name.as_str();
        let collectd_topic = match CollectdTopic::from_str(topic) {
            Ok(collectd_topic) => collectd_topic,
            Err(_) => {
                return Err(CollectdError::InvalidMeasurementTopic(topic.into()));
            }
        };

        let payload = mqtt_message.payload_str().map_err(|_err| {
            CollectdError::NonUTF8MeasurementPayload(mqtt_message.payload_raw().into())
        })?;

        let collectd_payload = CollectdPayload::parse_from(payload)
            .map_err(|err| CollectdError::InvalidMeasurementPayload(topic.into(), err))?;

        let num_measurements = collectd_payload.metric_values.len();
        let mut collectd_mssages: Vec<CollectdMessage> = Vec::with_capacity(num_measurements);
        let mut i = 1;
        for m in collectd_payload.metric_values.iter() {
            let mut metric_key = collectd_topic.metric_key.to_string();
            if num_measurements > 1 {
                metric_key += "_";
                metric_key += &i.to_string();
            }
            collectd_mssages.push(CollectdMessage {
                metric_group_key: collectd_topic.metric_group_key.to_string(),
                metric_key,
                timestamp: collectd_payload.timestamp(),
                metric_value: *m,
            });
            i = i + 1;
        }
        Ok(collectd_mssages)
    }
}

#[derive(Debug, Eq, PartialEq, Hash)]
pub struct CollectdTopic<'a> {
    metric_group_key: &'a str,
    metric_key: &'a str,
}

#[derive(Debug)]
struct InvalidCollectdTopicName;

impl<'a> CollectdTopic<'a> {
    fn from_str(topic_name: &'a str) -> Result<Self, InvalidCollectdTopicName> {
        let mut iter = topic_name.split('/');
        let _collectd_prefix = iter.next().ok_or(InvalidCollectdTopicName)?;
        let _hostname = iter.next().ok_or(InvalidCollectdTopicName)?;
        let metric_group_key = iter.next().ok_or(InvalidCollectdTopicName)?;
        let metric_key = iter.next().ok_or(InvalidCollectdTopicName)?;

        match iter.next() {
            None => Ok(CollectdTopic {
                metric_group_key,
                metric_key,
            }),
            Some(_) => Err(InvalidCollectdTopicName),
        }
    }
}

#[derive(Debug)]
struct CollectdPayload {
    timestamp: f64,
    metric_values: Vec<f64>,
}

#[derive(thiserror::Error, Debug)]
pub enum CollectdPayloadError {
    #[error("Invalid payload: {0}. Expected payload format: <timestamp>:<value>")]
    InvalidMeasurementPayloadFormat(String),

    #[error("Invalid measurement timestamp: {0}. Epoch time value expected")]
    InvalidMeasurementTimestamp(String),

    #[error("Invalid measurement value: {0}. Must be a number")]
    InvalidMeasurementValue(String),
}

impl CollectdPayload {
    fn parse_from(payload: &str) -> Result<Self, CollectdPayloadError> {
        let msg: Vec<&str> = payload.split(':').collect();
        dbg!(&msg);
        let mut msg_iter = msg.iter();

        if msg.len() <= 1 {
            return Err(CollectdPayloadError::InvalidMeasurementPayloadFormat(
                payload.to_string(),
            ));
        }

        let timestamp = *msg_iter.next().ok_or_else(|| {
            CollectdPayloadError::InvalidMeasurementPayloadFormat(payload.to_string())
        })?;

        let timestamp = timestamp.parse::<f64>().map_err(|_err| {
            CollectdPayloadError::InvalidMeasurementTimestamp(timestamp.to_string())
        })?;

        let mut metric_values: Vec<f64> = Vec::with_capacity(msg.len());

        for _i in 1..msg.len() {
            let value = *msg_iter.next().ok_or_else(|| {
                CollectdPayloadError::InvalidMeasurementPayloadFormat(payload.to_string())
            })?;

            let value = value
                .parse::<f64>()
                .map_err(|_err| CollectdPayloadError::InvalidMeasurementValue(value.to_string()))?;

            metric_values.push(value);
        }

        Ok(CollectdPayload {
            timestamp,
            metric_values,
        })
    }

    pub fn timestamp(&self) -> DateTime<Utc> {
        let timestamp = self.timestamp.trunc() as i64;
        let nanoseconds = (self.timestamp.fract() * 1.0e9) as u32;
        DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp, nanoseconds), Utc)
    }
}

impl Batchable for CollectdMessage {
    type Key = String;

    fn key(&self) -> Self::Key {
        format!("{}/{}", &self.metric_group_key, &self.metric_key)
    }

    fn event_time(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Index;

    use assert_matches::assert_matches;
    use chrono::TimeZone;
    use mqtt_client::Topic;

    use super::*;

    #[test]
    fn collectd_message_parsing() {
        let topic = Topic::new("collectd/localhost/temperature/value").unwrap();
        let mqtt_message = Message::new(&topic, "123456789:32.5");

        let collectd_message = CollectdMessage::parse_from(&mqtt_message).unwrap();

        assert_eq!(collectd_message.index(0).metric_group_key, "temperature");
        assert_eq!(collectd_message.index(0).metric_key, "value");
        assert_eq!(
            collectd_message.index(0).timestamp,
            Utc.ymd(1973, 11, 29).and_hms_milli(21, 33, 09, 0)
        );
        assert_eq!(collectd_message.index(0).metric_value, 32.5);
    }

    #[test]
    fn collectd_message_parsing_multi_valued_measurement() {
        let topic = Topic::new("collectd/localhost/temperature/value").unwrap();
        let mqtt_message = Message::new(&topic, "123456789:32.5:45.2");

        let collectd_message = CollectdMessage::parse_from(&mqtt_message).unwrap();

        assert_eq!(collectd_message.index(0).metric_group_key, "temperature");
        assert_eq!(collectd_message.index(0).metric_key, "value_1");
        assert_eq!(
            collectd_message.index(0).timestamp,
            Utc.ymd(1973, 11, 29).and_hms_milli(21, 33, 09, 0)
        );
        assert_eq!(collectd_message.index(0).metric_value, 32.5);

        assert_eq!(collectd_message.index(1).metric_group_key, "temperature");
        assert_eq!(collectd_message.index(1).metric_key, "value_2");
        assert_eq!(
            collectd_message.index(1).timestamp,
            Utc.ymd(1973, 11, 29).and_hms_milli(21, 33, 09, 0)
        );
        assert_eq!(collectd_message.index(1).metric_value, 45.2);
    }

    #[test]
    fn collectd_null_terminated_message_parsing() {
        let topic = Topic::new("collectd/localhost/temperature/value").unwrap();
        let mqtt_message = Message::new(&topic, "123456789.125:32.5\u{0}");

        let collectd_message = CollectdMessage::parse_from(&mqtt_message).unwrap();

        assert_eq!(collectd_message.index(0).metric_group_key, "temperature");
        assert_eq!(collectd_message.index(0).metric_key, "value");
        assert_eq!(
            collectd_message.index(0).timestamp,
            Utc.ymd(1973, 11, 29).and_hms_milli(21, 33, 09, 125)
        );
        assert_eq!(collectd_message.index(0).metric_value, 32.5);
    }

    #[test]
    fn invalid_collectd_message_topic() {
        let topic = Topic::new("collectd/less/level").unwrap();
        let mqtt_message = Message::new(&topic, "123456789:32.5");

        let result = CollectdMessage::parse_from(&mqtt_message);

        assert_matches!(result, Err(CollectdError::InvalidMeasurementTopic(_)));
    }

    #[test]
    fn invalid_collectd_message_payload() {
        let topic = Topic::new("collectd/host/group/key").unwrap();
        let invalid_collectd_message = Message::new(&topic, "123456789");

        let result = CollectdMessage::parse_from(&invalid_collectd_message);

        assert_matches!(result, Err(CollectdError::InvalidMeasurementPayload(_, _)));
    }

    #[test]
    fn invalid_collectd_topic_less_levels() {
        let result = CollectdTopic::from_str("collectd/less/levels");

        assert_matches!(result, Err(InvalidCollectdTopicName));
    }

    #[test]
    fn invalid_collectd_topic_more_levels() {
        let result = CollectdTopic::from_str("collectd/more/levels/than/needed");

        assert_matches!(result, Err(InvalidCollectdTopicName));
    }

    #[test]
    fn invalid_collectd_payload_no_separator() {
        let payload = "123456789";
        let result = CollectdPayload::parse_from(payload);

        assert_matches!(
            result,
            Err(CollectdPayloadError::InvalidMeasurementPayloadFormat(_))
        );
    }

    #[test]
    fn invalid_collectd_metric_value() {
        let payload = "123456789:abc";
        let result = CollectdPayload::parse_from(payload);

        assert_matches!(
            result,
            Err(CollectdPayloadError::InvalidMeasurementValue(_))
        );
    }

    #[test]
    fn valid_collectd_multivalue_metric() {
        let payload = "123456789:1234:5678";
        let result = CollectdPayload::parse_from(payload).unwrap();

        assert_eq!(result.timestamp, 123456789.0);
        assert_eq!(result.metric_values, vec![1234.0, 5678.0]);
    }

    #[test]
    fn invalid_collectd_metric_timestamp() {
        let payload = "abc:98.6";
        let result = CollectdPayload::parse_from(payload);

        assert_matches!(
            result,
            Err(CollectdPayloadError::InvalidMeasurementTimestamp(_))
        );
    }

    #[test]
    fn very_large_metric_value() {
        let payload: String = format!("123456789:{}", u128::MAX);
        let collectd_payload = CollectdPayload::parse_from(payload.as_str()).unwrap();

        assert_eq!(*collectd_payload.metric_values.index(0), u128::MAX as f64);
    }

    #[test]
    fn very_small_metric_value() {
        let payload: String = format!("123456789:{}", i128::MIN);
        let collectd_payload = CollectdPayload::parse_from(payload.as_str()).unwrap();

        assert_eq!(*collectd_payload.metric_values.index(0), i128::MIN as f64);
    }
}
