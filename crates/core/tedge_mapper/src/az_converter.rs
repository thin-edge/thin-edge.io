use crate::converter::*;
use crate::error::*;
use crate::size_threshold::SizeThreshold;
use clock::Clock;
use mqtt_client::Message;
use thin_edge_json::serialize::ThinEdgeJsonSerializer;

pub struct AzureConverter {
    pub(crate) add_timestamp: bool,
    pub(crate) clock: Box<dyn Clock>,
    pub(crate) size_threshold: SizeThreshold,
    pub(crate) mapper_config: MapperConfig,
}

impl AzureConverter {
    pub fn new(add_timestamp: bool, clock: Box<dyn Clock>, size_threshold: SizeThreshold) -> Self {
        let mapper_config = MapperConfig {
            in_topic_filter: make_valid_topic_filter_or_panic("tedge/measurements"),
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
}

impl Converter for AzureConverter {
    type Error = ConversionError;

    fn get_mapper_config(&self) -> &MapperConfig {
        &self.mapper_config
    }

    fn try_convert(&mut self, input: &Message) -> Result<Vec<Message>, Self::Error> {
        let input = input.payload_str()?;
        let () = self.size_threshold.validate(input)?;
        let default_timestamp = self.add_timestamp.then(|| self.clock.now());
        let mut serializer = ThinEdgeJsonSerializer::new_with_timestamp(default_timestamp);
        let () = thin_edge_json::parser::parse_str(input, &mut serializer)?;

        let payload = serializer.into_string()?;
        Ok(vec![(Message::new(&self.mapper_config.out_topic, payload))])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::size_threshold::SizeThresholdExceeded;
    use assert_json_diff::*;
    use assert_matches::*;
    use chrono::{FixedOffset, TimeZone};
    use mqtt_client::Topic;
    use serde_json::json;

    struct TestClock;

    impl Clock for TestClock {
        fn now(&self) -> clock::Timestamp {
            FixedOffset::east(5 * 3600).ymd(2021, 4, 8).and_hms(0, 0, 0)
        }
    }

    #[test]
    fn converting_invalid_json_is_invalid() {
        let mut converter =
            AzureConverter::new(false, Box::new(TestClock), SizeThreshold(255 * 1024));

        let input = "This is not Thin Edge JSON";
        let result = converter.try_convert(&new_tedge_message(input));

        assert_matches!(result, Err(ConversionError::FromThinEdgeJsonParser(_)))
    }

    fn new_tedge_message(input: &str) -> Message {
        Message::new(&Topic::new_unchecked("tedge/measurements"), input)
    }

    fn extract_first_message_payload(mut messages: Vec<Message>) -> String {
        messages.pop().unwrap().payload_str().unwrap().to_string()
    }

    #[test]
    fn converting_input_without_timestamp_produces_output_without_timestamp_given_add_timestamp_is_false(
    ) {
        let mut converter =
            AzureConverter::new(false, Box::new(TestClock), SizeThreshold(255 * 1024));

        let input = r#"{
            "temperature": 23.0
         }"#;

        let expected_output = json!({
            "temperature": 23.0
        });

        let output = converter.convert(&new_tedge_message(input));

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_input_with_timestamp_produces_output_with_timestamp_given_add_timestamp_is_false()
    {
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

        let output = converter.convert(&new_tedge_message(input));

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_input_with_timestamp_produces_output_with_timestamp_given_add_timestamp_is_true()
    {
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

        let output = converter.convert(&new_tedge_message(input));

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_input_without_timestamp_produces_output_with_timestamp_given_add_timestamp_is_true(
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

        let output = converter.convert(&new_tedge_message(input));

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn exceeding_threshold_returns_error() {
        let mut converter = AzureConverter::new(false, Box::new(TestClock), SizeThreshold(1));

        let input = "ABC";
        let result = converter.try_convert(&new_tedge_message(input));

        assert_matches!(
            result,
            Err(ConversionError::FromSizeThresholdExceeded(
                SizeThresholdExceeded {
                    actual_size: 3,
                    threshold: 1
                }
            ))
        );
    }
}
