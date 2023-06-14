use crate::error::ConversionError;
use crate::size_threshold::SizeThreshold;
use clock::Clock;
use log::error;
use std::convert::Infallible;
use tedge_actors::Converter;
use tedge_api::serialize::ThinEdgeJsonSerializer;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;

const AZ_MQTT_THRESHOLD: usize = 1024 * 128;

#[derive(Debug)]
pub struct MapperConfig {
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
    pub fn new(add_timestamp: bool, clock: Box<dyn Clock>) -> Self {
        let mapper_config = MapperConfig {
            out_topic: Topic::new_unchecked("az/messages/events/"),
            errors_topic: Topic::new_unchecked("tedge/errors"),
        };

        let size_threshold = SizeThreshold(AZ_MQTT_THRESHOLD);
        AzureConverter {
            add_timestamp,
            clock,
            size_threshold,
            mapper_config,
        }
    }

    pub fn with_threshold(self, size_threshold: SizeThreshold) -> Self {
        Self {
            size_threshold,
            ..self
        }
    }

    fn try_convert(&mut self, input: &MqttMessage) -> Result<Vec<MqttMessage>, ConversionError> {
        self.size_threshold.validate(input)?;
        let default_timestamp = self.add_timestamp.then(|| self.clock.now());
        let mut serializer = ThinEdgeJsonSerializer::new_with_timestamp(default_timestamp);
        tedge_api::parser::parse_str(input.payload_str()?, &mut serializer)?;

        let payload = serializer.into_string()?;
        Ok(vec![
            (MqttMessage::new(&self.mapper_config.out_topic, payload)),
        ])
    }

    fn wrap_errors(
        &self,
        messages_or_err: Result<Vec<MqttMessage>, ConversionError>,
    ) -> Vec<MqttMessage> {
        messages_or_err.unwrap_or_else(|error| vec![self.new_error_message(error)])
    }

    fn new_error_message(&self, error: ConversionError) -> MqttMessage {
        error!("Mapping error: {}", error);
        MqttMessage::new(&self.mapper_config.errors_topic, error.to_string())
    }
}

impl Converter for AzureConverter {
    type Input = MqttMessage;
    type Output = MqttMessage;
    type Error = Infallible;

    fn convert(&mut self, input: &Self::Input) -> Result<Vec<Self::Output>, Self::Error> {
        let messages_or_err = self.try_convert(input);
        Ok(self.wrap_errors(messages_or_err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_json_diff::*;
    use assert_matches::*;
    use serde_json::json;
    use time::macros::datetime;

    struct TestClock;

    impl Clock for TestClock {
        fn now(&self) -> clock::Timestamp {
            datetime!(2021-04-08 00:00:00 +05:00)
        }
    }

    fn new_tedge_message(input: &str) -> MqttMessage {
        MqttMessage::new(&Topic::new_unchecked("tedge/measurements"), input)
    }

    fn extract_first_message_payload(mut messages: Vec<MqttMessage>) -> String {
        messages.pop().unwrap().payload_str().unwrap().to_string()
    }

    #[test]
    fn convert_error() {
        let mut converter = AzureConverter::new(true, Box::new(TestClock));

        let input = "Invalid JSON";

        let output = converter.convert(&new_tedge_message(input)).unwrap();

        assert_eq!(output.first().unwrap().topic.name, "tedge/errors");
        assert_eq!(
            extract_first_message_payload(output),
            "Invalid JSON: expected value at line 1 column 1: `Invalid JSON\n`"
        );
    }

    #[test]
    fn try_convert_invalid_json_returns_error() {
        let mut converter = AzureConverter::new(false, Box::new(TestClock));

        let input = "This is not Thin Edge JSON";
        let result = converter.try_convert(&new_tedge_message(input));

        assert_matches!(result, Err(ConversionError::FromThinEdgeJsonParser(_)))
    }

    #[test]
    fn try_convert_exceeding_threshold_returns_error() {
        let mut converter =
            AzureConverter::new(false, Box::new(TestClock)).with_threshold(SizeThreshold(1));

        let _topic = "tedge/measurements".to_string();
        let input = "ABC";
        let result = converter.try_convert(&new_tedge_message(input));

        assert_matches!(
            result,
            Err(ConversionError::SizeThresholdExceeded {
                topic: _topic,
                actual_size: 3,
                threshold: 1
            })
        );
    }

    #[test]
    fn converting_input_without_timestamp_produces_output_without_timestamp_given_add_timestamp_is_false(
    ) {
        let mut converter = AzureConverter::new(false, Box::new(TestClock));

        let input = r#"{
            "temperature": 23.0
         }"#;

        let expected_output = json!({
            "temperature": 23.0
        });

        let output = converter.convert(&new_tedge_message(input)).unwrap();

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_input_with_timestamp_produces_output_with_timestamp_given_add_timestamp_is_false()
    {
        let mut converter = AzureConverter::new(false, Box::new(TestClock));

        let input = r#"{
            "time" : "2013-06-22T17:03:14.000+02:00",
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "time" : "2013-06-22T17:03:14+02:00",
            "temperature": 23.0
        });

        let output = converter.convert(&new_tedge_message(input)).unwrap();

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_input_with_timestamp_produces_output_with_timestamp_given_add_timestamp_is_true()
    {
        let mut converter = AzureConverter::new(true, Box::new(TestClock));

        let input = r#"{
            "time" : "2013-06-22T17:03:14.000+02:00",
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "time" : "2013-06-22T17:03:14+02:00",
            "temperature": 23.0
        });

        let output = converter.convert(&new_tedge_message(input)).unwrap();

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_input_without_timestamp_produces_output_with_timestamp_given_add_timestamp_is_true(
    ) {
        let mut converter = AzureConverter::new(true, Box::new(TestClock));

        let input = r#"{
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "temperature": 23.0,
            "time": "2021-04-08T00:00:00+05:00"
        });

        let output = converter.convert(&new_tedge_message(input)).unwrap();

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }
}
