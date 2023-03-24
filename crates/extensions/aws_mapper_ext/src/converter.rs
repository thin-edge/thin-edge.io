use tedge_mapper_core::converter::*;
use tedge_mapper_core::error::*;
use tedge_mapper_core::size_threshold::SizeThreshold;

use async_trait::async_trait;
use clock::Clock;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use serde_json::Map;
use serde_json::Value;
use tedge_api::serialize::ThinEdgeJsonSerializer;

pub struct AwsConverter {
    pub(crate) add_timestamp: bool,
    pub(crate) clock: Box<dyn Clock>,
    pub(crate) size_threshold: SizeThreshold,
    pub(crate) mapper_config: MapperConfig,
}

impl AwsConverter {
    pub fn new(add_timestamp: bool, clock: Box<dyn Clock>, size_threshold: SizeThreshold) -> Self {
        let mapper_config = MapperConfig {
            in_topic_filter: Self::in_topic_filter(),
            out_topic: make_valid_topic_or_panic("aws/td/measurements"),
            errors_topic: make_valid_topic_or_panic("tedge/errors"),
        };
        AwsConverter {
            add_timestamp,
            clock,
            size_threshold,
            mapper_config,
        }
    }

    pub fn in_topic_filter() -> TopicFilter {
        vec![
            "tedge/measurements",
            "tedge/measurements/+",
            "tedge/health",
            "tedge/health/+",
            "tedge/events/+",
            "tedge/events/+/+",
            "tedge/alarms/+/+",
            "tedge/alarms/+/+/+",
        ]
        .try_into()
        .unwrap()
    }
}

#[async_trait]
impl Converter for AwsConverter {
    type Error = ConversionError;

    fn get_mapper_config(&self) -> &MapperConfig {
        &self.mapper_config
    }

    async fn try_convert(&mut self, input: &Message) -> Result<Vec<Message>, Self::Error> {
        let default_timestamp = self.add_timestamp.then(|| self.clock.now());

        // serialize with ThinEdgeJson for measurements, for alarms and events just add the timestamp
        let payload = if input.topic.name.starts_with("tedge/measurements") {
            let mut serializer = ThinEdgeJsonSerializer::new_with_timestamp(default_timestamp);
            tedge_api::parser::parse_str(input.payload_str()?, &mut serializer)?;

            serializer.into_string()?
        } else if input.topic.name.starts_with("tedge/events")
            || input.topic.name.starts_with("tedge/alarms")
            || input.topic.name.starts_with("tedge/health")
        {
            let mut payload_json: Map<String, Value> =
                serde_json::from_slice(input.payload.as_ref())?;

            if let Some(timestamp) = default_timestamp {
                let timestamp = timestamp
                    .format(&time::format_description::well_known::Rfc3339)?
                    .as_str()
                    .into();
                payload_json.entry("time").or_insert(timestamp);
            }

            serde_json::to_string(&payload_json)?
        } else {
            return Ok(vec![]);
        };

        let topic_suffix = match input.topic.name.split_once('/') {
            Some((_, topic_suffix)) => topic_suffix,
            None => return Ok(vec![]),
        };

        let out_topic = Topic::new(&format!("aws/td/{topic_suffix}"))?;

        let output = Message::new(&out_topic, payload);
        self.size_threshold.validate(&output)?;
        Ok(vec![(output)])
    }
}

#[cfg(test)]
mod tests {
    use crate::converter::AwsConverter;
    use tedge_mapper_core::converter::*;
    use tedge_mapper_core::error::ConversionError;
    use tedge_mapper_core::size_threshold::SizeThreshold;

    use assert_json_diff::*;
    use assert_matches::*;
    use clock::Clock;
    use mqtt_channel::Message;
    use mqtt_channel::Topic;
    use serde_json::json;
    use time::macros::datetime;

    struct TestClock;

    impl Clock for TestClock {
        fn now(&self) -> clock::Timestamp {
            datetime!(2021-04-08 00:00:00 +05:00)
        }
    }

    #[tokio::test]
    async fn converting_invalid_json_is_invalid() {
        let mut converter =
            AwsConverter::new(false, Box::new(TestClock), SizeThreshold(128 * 1024));

        let input = "This is not Thin Edge JSON";
        let result = converter.try_convert(&new_tedge_message(input)).await;

        assert_matches!(result, Err(ConversionError::FromThinEdgeJsonParser(_)))
    }

    fn new_tedge_message(input: &str) -> Message {
        Message::new(&Topic::new_unchecked("tedge/measurements"), input)
    }

    fn extract_first_message_payload(mut messages: Vec<Message>) -> String {
        messages.pop().unwrap().payload_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn converting_input_without_timestamp_produces_output_without_timestamp_given_add_timestamp_is_false(
    ) {
        let mut converter =
            AwsConverter::new(false, Box::new(TestClock), SizeThreshold(128 * 1024));

        let input = r#"{
            "temperature": 23.0
         }"#;

        let expected_output = json!({
            "temperature": 23.0
        });

        let output = converter.convert(&new_tedge_message(input)).await;

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn converting_input_with_timestamp_produces_output_with_timestamp_given_add_timestamp_is_false(
    ) {
        let mut converter =
            AwsConverter::new(false, Box::new(TestClock), SizeThreshold(128 * 1024));

        let input = r#"{
            "time" : "2013-06-22T17:03:14.000+02:00",
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "time" : "2013-06-22T17:03:14+02:00",
            "temperature": 23.0
        });

        let output = converter.convert(&new_tedge_message(input)).await;

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn converting_input_with_timestamp_produces_output_with_timestamp_given_add_timestamp_is_true(
    ) {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), SizeThreshold(128 * 1024));

        let input = r#"{
            "time" : "2013-06-22T17:03:14.000+02:00",
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "time" : "2013-06-22T17:03:14+02:00",
            "temperature": 23.0
        });

        let output = converter.convert(&new_tedge_message(input)).await;

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn converting_input_without_timestamp_produces_output_with_timestamp_given_add_timestamp_is_true(
    ) {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), SizeThreshold(128 * 1024));

        let input = r#"{
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "temperature": 23.0,
            "time": "2021-04-08T00:00:00+05:00"
        });

        let output = converter.convert(&new_tedge_message(input)).await;

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn exceeding_threshold_returns_error() {
        let mut converter = AwsConverter::new(false, Box::new(TestClock), SizeThreshold(1));

        let _topic = "tedge/measurements".to_string();
        let input = r#"{"temperature": 21.3}"#;
        let _input_size = input.len();
        let result = converter.try_convert(&new_tedge_message(input)).await;

        assert_matches!(
            result,
            Err(ConversionError::SizeThresholdExceeded {
                topic: _topic,
                actual_size: _input_size,
                threshold: 1
            })
        );
    }
}
