use clock::Clock;
use log::error;
use serde_json::Map;
use serde_json::Value;
use std::convert::Infallible;
use tedge_actors::Converter;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;

use crate::error::ConversionError;
use crate::size_threshold::SizeThreshold;

const AWS_MQTT_THRESHOLD: usize = 1024 * 255;
const MOSQUITTO_BRIDGE_TOPIC_ID: &str = "device/main/service/mosquitto-aws-bridge";

pub struct AwsConverter {
    pub(crate) add_timestamp: bool,
    pub(crate) clock: Box<dyn Clock>,
    pub(crate) size_threshold: SizeThreshold,
    pub mqtt_schema: MqttSchema,
    pub topic_root: String,
}

impl AwsConverter {
    pub fn new(add_timestamp: bool, clock: Box<dyn Clock>, topic_root: &str) -> Self {
        let size_threshold = SizeThreshold(AWS_MQTT_THRESHOLD);
        AwsConverter {
            add_timestamp,
            clock,
            size_threshold,
            mqtt_schema: MqttSchema::default(),
            topic_root: topic_root.into(),
        }
    }

    pub fn with_threshold(self, size_threshold: SizeThreshold) -> Self {
        Self {
            size_threshold,
            ..self
        }
    }

    fn try_convert(&mut self, input: &MqttMessage) -> Result<Vec<MqttMessage>, ConversionError> {
        let messages = match self.mqtt_schema.entity_channel_of(&input.topic) {
            Ok((source, channel)) => self.try_convert_te_topics(source, channel, input),
            Err(_) => Ok(vec![]),
        }?;

        for message in &messages {
            self.size_threshold.validate(message)?;
        }

        Ok(messages)
    }

    fn try_convert_te_topics(
        &mut self,
        source: EntityTopicId,
        channel: Channel,
        input: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        // don't convert mosquitto bridge notification topic
        // https://github.com/thin-edge/thin-edge.io/issues/2236
        if source.as_str() == MOSQUITTO_BRIDGE_TOPIC_ID {
            return Ok(vec![]);
        }

        match channel {
            Channel::Measurement {
                measurement_type: type_name,
            }
            | Channel::Event {
                event_type: type_name,
            }
            | Channel::Alarm {
                alarm_type: type_name,
            } => self.convert_telemetry_message(input, source, &type_name),

            Channel::Health => self.convert_health_message(&source, &channel, input),

            _ => Ok(vec![]),
        }
    }

    fn convert_health_message(
        &self,
        source: &EntityTopicId,
        channel: &Channel,
        input: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        match channel {
            Channel::Health => (),
            _ => panic!("Not a health message"),
        }

        let source = normalize_name(source);
        let out_topic = format!("aws/td/{source}/status/health");
        let payload = self.with_time_stamp(input)?;
        let output = MqttMessage::new(&Topic::new(&out_topic).unwrap(), payload);

        Ok(vec![output])
    }

    fn convert_telemetry_message(
        &mut self,
        input: &MqttMessage,
        source: EntityTopicId,
        telemetry_type: &String,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let payload = self.with_time_stamp(input)?;
        let source = normalize_name(&source);
        // XXX: should match on `Channel` instead
        let out_topic = match input.topic.name.split('/').collect::<Vec<_>>()[..] {
            [_, _, _, _, _, "m", _] => {
                Topic::new_unchecked(&format!("aws/td/{source}/m/{telemetry_type}"))
            }
            [_, _, _, _, _, "e", _] => {
                Topic::new_unchecked(&format!("aws/td/{source}/e/{telemetry_type}"))
            }
            [_, _, _, _, _, "a", _] => {
                Topic::new_unchecked(&format!("aws/td/{source}/a/{telemetry_type}"))
            }
            _ => return Ok(vec![]),
        };

        let output = MqttMessage::new(&out_topic, payload);
        self.size_threshold.validate(&output)?;
        Ok(vec![output])
    }

    fn with_time_stamp(&self, input: &MqttMessage) -> Result<String, ConversionError> {
        let default_timestamp = self.add_timestamp.then(|| self.clock.now());
        let mut payload_json: Map<String, Value> =
            serde_json::from_slice(input.payload.as_bytes())?;

        if let Some(timestamp) = default_timestamp {
            let timestamp = timestamp
                .format(&time::format_description::well_known::Rfc3339)?
                .as_str()
                .into();
            payload_json.entry("time").or_insert(timestamp);
        }
        Ok(serde_json::to_string(&payload_json)?)
    }

    fn wrap_errors(
        &self,
        messages_or_err: Result<Vec<MqttMessage>, ConversionError>,
    ) -> Vec<MqttMessage> {
        messages_or_err.unwrap_or_else(|error| vec![self.new_error_message(error)])
    }

    fn new_error_message(&self, error: ConversionError) -> MqttMessage {
        error!("Mapping error: {}", error);
        MqttMessage::new(
            &Topic::new_unchecked(&format!("{0}/errors", self.topic_root)),
            error.to_string(),
        )
    }
}

// We need to reduce the number of levels in the topic because AWS IoT only supports topics with 7
// slashes (`/`)
//
// Ref: https://docs.aws.amazon.com/general/latest/gr/iot-core.html -> "Maximum number of slashes in
// topic and topic filter"
fn normalize_name(source: &EntityTopicId) -> String {
    let parts: Vec<&str> = source.as_str().split('/').collect();
    parts
        .iter()
        .filter(|&&part| !part.is_empty())
        .cloned()
        .collect::<Vec<&str>>()
        .join(":")
}

impl Converter for AwsConverter {
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
    use crate::error::ConversionError::FromSerdeJson;
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
        MqttMessage::new(&Topic::new_unchecked("te/device/main///m/"), input)
    }

    fn extract_first_message_payload(mut messages: Vec<MqttMessage>) -> String {
        messages.pop().unwrap().payload_str().unwrap().to_string()
    }

    #[test]
    fn convert_error() {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = "Invalid JSON";

        let output = converter.convert(&new_tedge_message(input)).unwrap();

        assert_eq!(output.first().unwrap().topic.name, "te/errors");
        assert_eq!(
            extract_first_message_payload(output),
            "expected value at line 1 column 1"
        );
    }

    #[test]
    fn try_convert_invalid_json_returns_error() {
        let mut converter = AwsConverter::new(false, Box::new(TestClock), "te");

        let input = "This is not Thin Edge JSON";
        let result = converter.try_convert(&new_tedge_message(input));
        assert_matches!(result, Err(FromSerdeJson(_)))
    }

    #[test]
    fn try_convert_exceeding_threshold_returns_error() {
        let mut converter =
            AwsConverter::new(false, Box::new(TestClock), "te").with_threshold(SizeThreshold(1));

        let _topic = "te/device/main///m/".to_string();
        let input = r#"{"temperature": 21.3}"#;
        let _input_size = input.len();
        let result = converter.try_convert(&new_tedge_message(input));

        assert_matches!(
            result,
            Err(ConversionError::SizeThresholdExceeded {
                topic: _topic,
                actual_size: _input_size,
                threshold: 1
            })
        );
    }

    #[test]
    fn converting_input_without_timestamp_produces_output_without_timestamp_given_add_timestamp_is_false(
    ) {
        let mut converter = AwsConverter::new(false, Box::new(TestClock), "te");

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
        let mut converter = AwsConverter::new(false, Box::new(TestClock), "te");

        let input = r#"{
            "time" : "2013-06-22T17:03:14.000+02:00",
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "time" : "2013-06-22T17:03:14.000+02:00",
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
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = r#"{
            "time" : "2013-06-22T17:03:14.000+02:00",
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "time" : "2013-06-22T17:03:14.000+02:00",
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
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = r#"{
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "temperature": 23.0,
            "time": "2021-04-08T00:00:00+05:00"
        });

        let output = converter.convert(&new_tedge_message(input)).unwrap();
        assert_json_eq!(output[0].topic.name, "aws/td/device:main/m/");

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_input_with_measurement_type() {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = r#"{
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "temperature": 23.0,
            "time": "2021-04-08T00:00:00+05:00"
        });
        let input = MqttMessage::new(&Topic::new_unchecked("te/device/main///m/test_type"), input);
        let output = converter.convert(&input).unwrap();
        assert_json_eq!(output[0].topic.name, "aws/td/device:main/m/test_type");

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_input_for_child_device_with_measurement_type() {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = r#"{
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "temperature": 23.0,
            "time": "2021-04-08T00:00:00+05:00"
        });
        let input = MqttMessage::new(
            &Topic::new_unchecked("te/device/child///m/test_type"),
            input,
        );
        let output = converter.convert(&input).unwrap();
        assert_json_eq!(output[0].topic.name, "aws/td/device:child/m/test_type");

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_input_for_main_device_service_with_measurement_type() {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = r#"{
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "temperature": 23.0,
            "time": "2021-04-08T00:00:00+05:00"
        });
        let input = MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/main_service/m/test_type"),
            input,
        );
        let output = converter.convert(&input).unwrap();
        assert_json_eq!(
            output[0].topic.name,
            "aws/td/device:main:service:main_service/m/test_type"
        );

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_input_for_child_device_service_with_measurement_type() {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = r#"{
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "temperature": 23.0,
            "time": "2021-04-08T00:00:00+05:00"
        });
        let input = MqttMessage::new(
            &Topic::new_unchecked("te/device/child/service/child_service/m/test_type"),
            input,
        );
        let output = converter.convert(&input).unwrap();
        assert_json_eq!(
            output[0].topic.name,
            "aws/td/device:child:service:child_service/m/test_type"
        );

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_bridge_health_status() {
        let mut converter = AwsConverter::new(false, Box::new(TestClock), "te");

        let input = "0";
        let result = converter.try_convert(&MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/mosquitto-aws-bridge/status/health"),
            input,
        ));
        let res = result.unwrap();
        assert!(res.is_empty());
    }

    #[test]
    fn converting_event_for_main_device() {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = r#"{
            "text": "I raised it",
            "time": "2021-04-23T19:00:00+05:00"
        }"#;

        let expected_output = json!({
            "text": "I raised it",
            "time": "2021-04-23T19:00:00+05:00"
        });

        let input = MqttMessage::new(
            &Topic::new_unchecked("te/device/main///e/event_type"),
            input,
        );
        let output = converter.convert(&input).unwrap();
        assert_json_eq!(output[0].topic.name, "aws/td/device:main/e/event_type");

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_event_for_child_device() {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = r#"{
            "text": "I raised it",
            "time": "2021-04-23T19:00:00+05:00"
        }"#;

        let expected_output = json!({
            "text": "I raised it",
            "time": "2021-04-23T19:00:00+05:00"
        });

        let input = MqttMessage::new(
            &Topic::new_unchecked("te/device/child///e/event_type"),
            input,
        );
        let output = converter.convert(&input).unwrap();
        assert_json_eq!(output[0].topic.name, "aws/td/device:child/e/event_type");

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_event_for_main_device_service() {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = r#"{
            "text": "I raised it",
            "time": "2021-04-23T19:00:00+05:00"
        }"#;

        let expected_output = json!({
            "text": "I raised it",
            "time": "2021-04-23T19:00:00+05:00"
        });

        let input = MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/main_service/e/event_type"),
            input,
        );
        let output = converter.convert(&input).unwrap();
        assert_json_eq!(
            output[0].topic.name,
            "aws/td/device:main:service:main_service/e/event_type"
        );

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_event_for_child_device_service() {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = r#"{
            "text": "I raised it",
            "time": "2021-04-23T19:00:00+05:00"
        }"#;

        let expected_output = json!({
            "text": "I raised it",
            "time": "2021-04-23T19:00:00+05:00"
        });

        let input = MqttMessage::new(
            &Topic::new_unchecked("te/device/child/service/child_service/e/event_type"),
            input,
        );
        let output = converter.convert(&input).unwrap();
        assert_json_eq!(
            output[0].topic.name,
            "aws/td/device:child:service:child_service/e/event_type"
        );

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_alarm_for_main_device() {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = r#"{
            "text":"I raised it",
            "severity":"critical",
            "time": "2021-04-23T19:00:00+05:00"
        }"#;

        let expected_output = json!({
            "text":"I raised it",
            "severity":"critical",
            "time": "2021-04-23T19:00:00+05:00"
        });

        let input = MqttMessage::new(
            &Topic::new_unchecked("te/device/main///a/alarm_type"),
            input,
        );
        let output = converter.convert(&input).unwrap();
        assert_json_eq!(output[0].topic.name, "aws/td/device:main/a/alarm_type");

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_alarm_for_main_service() {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = r#"{
            "text":"I raised it",
            "severity":"critical",
            "time": "2021-04-23T19:00:00+05:00"
        }"#;

        let expected_output = json!({
            "text":"I raised it",
            "severity":"critical",
            "time": "2021-04-23T19:00:00+05:00"
        });

        let input = MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/cservice/a/alarm_type"),
            input,
        );
        let output = converter.convert(&input).unwrap();
        assert_json_eq!(
            output[0].topic.name,
            "aws/td/device:main:service:cservice/a/alarm_type"
        );

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_alarm_for_child_device() {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = r#"{
            "text":"I raised it",
            "severity":"critical",
            "time": "2021-04-23T19:00:00+05:00"
        }"#;

        let expected_output = json!({
            "text":"I raised it",
            "severity":"critical",
            "time": "2021-04-23T19:00:00+05:00"
        });

        let input = MqttMessage::new(
            &Topic::new_unchecked("te/device/child///a/alarm_type"),
            input,
        );
        let output = converter.convert(&input).unwrap();
        assert_json_eq!(output[0].topic.name, "aws/td/device:child/a/alarm_type");

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_alarm_for_child_service() {
        let mut converter = AwsConverter::new(true, Box::new(TestClock), "te");

        let input = r#"{
            "text":"I raised it",
            "severity":"critical",
            "time": "2021-04-23T19:00:00+05:00"
        }"#;

        let expected_output = json!({
            "text":"I raised it",
            "severity":"critical",
            "time": "2021-04-23T19:00:00+05:00"
        });

        let input = MqttMessage::new(
            &Topic::new_unchecked("te/device/child/service/cservice/a/alarm_type"),
            input,
        );
        let output = converter.convert(&input).unwrap();
        assert_json_eq!(
            output[0].topic.name,
            "aws/td/device:child:service:cservice/a/alarm_type"
        );

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[test]
    fn converting_service_health_status_up_message() {
        let mut converter = AwsConverter::new(false, Box::new(TestClock), "te");

        let input = r#"{"pid":1234,"status":"up"}"#;
        let result = converter.try_convert(&MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/test-service/status/health"),
            input,
        ));

        let expected_msg = MqttMessage::new(
            &Topic::new_unchecked("aws/td/device:main:service:test-service/status/health"),
            input,
        );
        let res = result.unwrap();
        assert_eq!(res[0], expected_msg);
    }

    #[test]
    fn converting_service_health_status_down_message() {
        let mut converter = AwsConverter::new(false, Box::new(TestClock), "te");

        let input = r#"{"pid":1234,"status":"up"}"#;
        let result = converter.try_convert(&MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/test-service/status/health"),
            input,
        ));

        let expected_msg = MqttMessage::new(
            &Topic::new_unchecked("aws/td/device:main:service:test-service/status/health"),
            input,
        );
        let res = result.unwrap();
        assert_eq!(res[0], expected_msg);
    }
}
