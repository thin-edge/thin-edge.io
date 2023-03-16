use batcher::Batchable;
use tedge_api::measurement::MeasurementVisitor;
use tedge_mqtt_ext::MqttMessage;
use time::Duration;
use time::OffsetDateTime;

#[derive(Debug)]
pub struct CollectdMessage {
    pub metric_group_key: String,
    pub metric_key: String,
    pub timestamp: OffsetDateTime,
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
        timestamp: OffsetDateTime,
    ) -> Self {
        Self {
            metric_group_key: metric_group_key.to_string(),
            metric_key: metric_key.to_string(),
            timestamp,
            metric_value,
        }
    }

    pub fn parse_from(mqtt_message: &MqttMessage) -> Result<Vec<Self>, CollectdError> {
        let topic = mqtt_message.topic.name.as_str();
        let collectd_topic = match CollectdTopic::from_str(topic) {
            Ok(collectd_topic) => collectd_topic,
            Err(_) => {
                return Err(CollectdError::InvalidMeasurementTopic(topic.into()));
            }
        };

        let payload = mqtt_message.payload_str().map_err(|_err| {
            CollectdError::NonUTF8MeasurementPayload(mqtt_message.payload_bytes().into())
        })?;

        let collectd_payload = CollectdPayload::parse_from(payload)
            .map_err(|err| CollectdError::InvalidMeasurementPayload(topic.into(), err))?;

        let num_measurements = collectd_payload.metric_values.len();
        let mut collectd_mssages: Vec<CollectdMessage> = Vec::with_capacity(num_measurements);

        for (i, value) in collectd_payload.metric_values.iter().enumerate() {
            let mut metric_key = collectd_topic.metric_key.to_string();
            // If there are multiple values, then create unique keys metric_key_val1, metric_key_val2 etc.
            if num_measurements > 1 {
                metric_key = format!("{}_val{}", metric_key, i + 1);
            }
            collectd_mssages.push(CollectdMessage {
                metric_group_key: collectd_topic.metric_group_key.to_string(),
                metric_key,
                timestamp: collectd_payload.timestamp(),
                metric_value: *value,
            });
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
#[allow(clippy::enum_variant_names)]
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
        let vec_len = msg.len();

        if vec_len <= 1 {
            return Err(CollectdPayloadError::InvalidMeasurementPayloadFormat(
                payload.to_string(),
            ));
        }

        // First element is always the timestamp
        let timestamp = msg[0].parse::<f64>().map_err(|_err| {
            CollectdPayloadError::InvalidMeasurementTimestamp(msg[0].to_string())
        })?;

        let metric_values = msg
            .into_iter()
            .skip(1)
            .map(|m| {
                m.parse::<f64>()
                    .map_err(|_err| CollectdPayloadError::InvalidMeasurementValue(m.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CollectdPayload {
            timestamp,
            metric_values,
        })
    }

    pub fn timestamp(&self) -> OffsetDateTime {
        let timestamp = self.timestamp.trunc() as i64;
        let nanoseconds = (self.timestamp.fract() * 1.0e9) as u32;
        OffsetDateTime::from_unix_timestamp(timestamp).unwrap()
            + Duration::nanoseconds(nanoseconds as i64)
    }
}

impl Batchable for CollectdMessage {
    type Key = String;

    fn key(&self) -> Self::Key {
        format!("{}/{}", &self.metric_group_key, &self.metric_key)
    }

    fn event_time(&self) -> OffsetDateTime {
        self.timestamp
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use std::ops::Index;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use time::macros::datetime;

    use super::*;

    #[test]
    fn collectd_message_parsing() {
        let topic = Topic::new_unchecked("collectd/localhost/temperature/value");
        let mqtt_message = MqttMessage::new(&topic, "123456789:32.5");

        let collectd_message = CollectdMessage::parse_from(&mqtt_message).unwrap();

        let CollectdMessage {
            metric_group_key,
            metric_key,
            timestamp,
            metric_value,
        } = collectd_message.index(0);
        assert_eq!(metric_group_key, "temperature");

        assert_eq!(metric_key, "value");
        assert_eq!(*timestamp, datetime!(1973-11-29 21:33:09.0 UTC));
        assert_eq!(*metric_value, 32.5);
    }

    #[test]
    fn collectd_message_parsing_multi_valued_measurement() {
        let topic = Topic::new("collectd/localhost/temperature/value").unwrap();
        let mqtt_message = MqttMessage::new(&topic, "123456789:32.5:45.2");

        let collectd_message = CollectdMessage::parse_from(&mqtt_message).unwrap();

        let CollectdMessage {
            metric_group_key,
            metric_key,
            timestamp,
            metric_value: _,
        } = collectd_message.index(0);
        assert_eq!(metric_group_key, "temperature");

        assert_eq!(metric_key, "value_val1");
        assert_eq!(*timestamp, datetime!(1973-11-29 21:33:09.0 UTC));

        let CollectdMessage {
            metric_group_key,
            metric_key,
            timestamp,
            metric_value,
        } = collectd_message.index(1);

        assert_eq!(metric_group_key, "temperature");
        assert_eq!(metric_key, "value_val2");
        assert_eq!(*timestamp, datetime!(1973-11-29 21:33:09.0 UTC));
        assert_eq!(*metric_value, 45.2);
    }

    #[test]
    fn collectd_null_terminated_message_parsing() {
        let topic = Topic::new("collectd/localhost/temperature/value").unwrap();
        let mqtt_message = MqttMessage::new(&topic, "123456789.125:32.5\u{0}");

        let collectd_message = CollectdMessage::parse_from(&mqtt_message).unwrap();

        let CollectdMessage {
            metric_group_key,
            metric_key,
            timestamp,
            metric_value,
        } = collectd_message.index(0);

        assert_eq!(metric_group_key, "temperature");
        assert_eq!(metric_key, "value");
        assert_eq!(*timestamp, datetime!(1973-11-29 21:33:09.125 UTC));
        assert_eq!(*metric_value, 32.5);
    }

    #[test]
    fn invalid_collectd_message_topic() {
        let topic = Topic::new("collectd/less/level").unwrap();
        let mqtt_message = MqttMessage::new(&topic, "123456789:32.5");

        let result = CollectdMessage::parse_from(&mqtt_message);

        assert_matches!(result, Err(CollectdError::InvalidMeasurementTopic(_)));
    }

    #[test]
    fn invalid_collectd_message_payload() {
        let topic = Topic::new("collectd/host/group/key").unwrap();
        let invalid_collectd_message = MqttMessage::new(&topic, "123456789");

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
    fn invalid_collectd_metric_multi_value() {
        let payload = "123456789:96.6:abc";
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
