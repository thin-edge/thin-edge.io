use clock::Clock;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use tedge_api::serialize::ThinEdgeJsonSerializer;
use tedge_api::topic::get_child_id_from_measurement_topic;
use tedge_mapper_core::error::*;
use tedge_mapper_core::size_threshold::SizeThreshold;
use tracing::error;

#[derive(Debug)]
pub struct MapperConfig {
    pub in_topic_filter: TopicFilter,
    pub out_topic: Topic,
    pub errors_topic: Topic,
}

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
        vec!["tedge/measurements", "tedge/measurements/+"]
            .try_into()
            .unwrap()
    }

    fn get_mapper_config(&self) -> &MapperConfig {
        &self.mapper_config
    }

    async fn try_convert(&mut self, input: &Message) -> Result<Vec<Message>, ConversionError> {
        let maybe_child_id = get_child_id_from_measurement_topic(&input.topic.name);

        let az_out_topic = match maybe_child_id {
            Some(child_id) => Topic::new_unchecked(&format!("az/messages/events/$.sub={child_id}")),
            None => self.mapper_config.out_topic.clone(),
        };

        self.size_threshold.validate(input)?;
        let default_timestamp = self.add_timestamp.then(|| self.clock.now());
        let mut serializer = ThinEdgeJsonSerializer::new_with_timestamp(default_timestamp);
        tedge_api::parser::parse_str(input.payload_str()?, &mut serializer)?;

        let payload = serializer.into_string()?;
        Ok(vec![(Message::new(&az_out_topic, payload))])
    }

    pub async fn convert(&mut self, input: &Message) -> Vec<Message> {
        let messages_or_err = self.try_convert(input).await;
        self.wrap_errors(messages_or_err)
    }

    fn wrap_errors(&self, messages_or_err: Result<Vec<Message>, ConversionError>) -> Vec<Message> {
        messages_or_err.unwrap_or_else(|error| vec![self.new_error_message(error)])
    }

    fn new_error_message(&self, error: ConversionError) -> Message {
        error!("Mapping error: {}", error);
        Message::new(&self.get_mapper_config().errors_topic, error.to_string())
    }
}

pub fn make_valid_topic_or_panic(topic_name: &str) -> Topic {
    Topic::new(topic_name).expect("Invalid topic name")
}

#[cfg(test)]
mod tests {
    use crate::converter::AzureConverter;
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
