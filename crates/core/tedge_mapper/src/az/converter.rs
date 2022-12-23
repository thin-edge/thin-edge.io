use crate::core::converter::*;
use crate::core::error::*;
use crate::core::size_threshold::SizeThreshold;

use async_trait::async_trait;
use clock::Clock;
use mqtt_channel::Message;
use mqtt_channel::TopicFilter;
use tedge_api::serialize::ThinEdgeJsonSerializer;

pub struct AzureConverter {
    pub(crate) add_timestamp: bool,
    pub(crate) clock: Box<dyn Clock>,
    pub(crate) size_threshold: SizeThreshold,
    pub(crate) mapper_config: MapperConfig,
}

impl AzureConverter {
    pub fn new(add_timestamp: bool, clock: Box<dyn Clock>, size_threshold: SizeThreshold) -> Self {
        let mapper_config = MapperConfig {
            in_topic_filter: Self::in_topic_filter(),
            out_topic: make_valid_topic_or_panic("az/messages/events/"),
            errors_topic: make_valid_topic_or_panic("tedge/errors"),
        };
        AzureConverter {
            add_timestamp,
            clock,
            size_threshold,
            mapper_config,
        }
    }

    pub fn in_topic_filter() -> TopicFilter {
        make_valid_topic_filter_or_panic("tedge/measurements")
    }
}

#[async_trait]
impl Converter for AzureConverter {
    type Error = ConversionError;

    fn get_mapper_config(&self) -> &MapperConfig {
        &self.mapper_config
    }

    async fn try_convert(&mut self, input: &Message) -> Result<Vec<Message>, Self::Error> {
        self.size_threshold.validate(input)?;
        let default_timestamp = self.add_timestamp.then(|| self.clock.now());
        let mut serializer = ThinEdgeJsonSerializer::new_with_timestamp(default_timestamp);
        tedge_api::parser::parse_str(input.payload_str()?, &mut serializer)?;

        let payload = serializer.into_string()?;
        Ok(vec![(Message::new(&self.mapper_config.out_topic, payload))])
    }
}

#[cfg(test)]
mod tests {
    use crate::az::converter::AzureConverter;
    use crate::core::converter::*;
    use crate::core::error::ConversionError;
    use crate::core::size_threshold::SizeThreshold;

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
            AzureConverter::new(false, Box::new(TestClock), SizeThreshold(255 * 1024));

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
            AzureConverter::new(false, Box::new(TestClock), SizeThreshold(255 * 1024));

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
            AzureConverter::new(false, Box::new(TestClock), SizeThreshold(255 * 1024));

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
        let mut converter =
            AzureConverter::new(true, Box::new(TestClock), SizeThreshold(255 * 1024));

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
        let mut converter =
            AzureConverter::new(true, Box::new(TestClock), SizeThreshold(255 * 1024));

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
        let mut converter = AzureConverter::new(false, Box::new(TestClock), SizeThreshold(1));

        let _topic = "tedge/measurements".to_string();
        let input = "ABC";
        let result = converter.try_convert(&new_tedge_message(input)).await;

        assert_matches!(
            result,
            Err(ConversionError::SizeThresholdExceeded {
                topic: _topic,
                actual_size: 3,
                threshold: 1
            })
        );
    }
}
