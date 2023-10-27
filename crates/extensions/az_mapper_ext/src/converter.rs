use crate::error::ConversionError;
use crate::size_threshold::SizeThreshold;
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

const AZ_MQTT_THRESHOLD: usize = 1024 * 128;
const MOSQUITTO_BRIDGE_TOPIC_ID: &str = "device/main/service/mosquitto-az-bridge";

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
    pub mqtt_schema: MqttSchema,
}

impl AzureConverter {
    pub fn new(add_timestamp: bool, clock: Box<dyn Clock>, topic_root: &str) -> Self {
        let mapper_config = MapperConfig {
            out_topic: Topic::new_unchecked("az/messages/events/"),
            errors_topic: Topic::new_unchecked(&format!("{topic_root}/errors")),
        };
        let size_threshold = SizeThreshold(AZ_MQTT_THRESHOLD);
        AzureConverter {
            add_timestamp,
            clock,
            size_threshold,
            mapper_config,
            mqtt_schema: MqttSchema::default(),
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
            Ok((entity, channel)) => self.try_convert_te_topics(input, &entity, channel),
            Err(_) => Ok(Vec::new()),
        }?;

        for message in &messages {
            self.size_threshold.validate(message)?;
        }

        Ok(messages)
    }

    // Todo: The device-id,telemetry kind (Meausrement/event/alarm) and telemetry type from the te topic has to be
    // used to push the telemetry messages on to specific azure topic.
    // For now all the messages will be sent over az/messages/events/ topic as this is the default mqtt topic for
    // sending the telemetry on to the azure iot hub.
    fn try_convert_te_topics(
        &mut self,
        input: &MqttMessage,
        entity: &EntityTopicId,
        channel: Channel,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        // don't convert mosquitto bridge notification topic
        // https://github.com/thin-edge/thin-edge.io/issues/2236
        if entity.as_str() == MOSQUITTO_BRIDGE_TOPIC_ID {
            return Ok(vec![]);
        }

        match &channel {
            Channel::Measurement { .. }
            | Channel::Event { .. }
            | Channel::Alarm { .. }
            | Channel::Health => {
                let payload = self.with_timestamp(input)?;
                let output = MqttMessage::new(&self.mapper_config.out_topic, payload);
                Ok(vec![output])
            }
            _ => Ok(vec![]),
        }
    }

    fn with_timestamp(&mut self, input: &MqttMessage) -> Result<String, ConversionError> {
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
    use crate::error::ConversionError::FromSerdeJson;
    use assert_json_diff::*;
    use assert_matches::*;
    use serde_json::json;
    use test_case::test_case;
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
        let mut converter = AzureConverter::new(true, Box::new(TestClock), "te");

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
        let mut converter = AzureConverter::new(false, Box::new(TestClock), "te");

        let input = "This is not Thin Edge JSON";
        let result = converter.try_convert(&new_tedge_message(input));

        assert_matches!(result, Err(FromSerdeJson(_)))
    }

    #[test]
    fn try_convert_exceeding_threshold_returns_error() {
        let mut converter =
            AzureConverter::new(false, Box::new(TestClock), "te").with_threshold(SizeThreshold(1));

        let _topic = "az/messages/events/".to_string();
        let input = r#"{
            "temperature": 23.0
         }"#;
        let result = converter.try_convert(&new_tedge_message(input));
        assert_matches!(
            result,
            Err(ConversionError::SizeThresholdExceeded {
                topic: _topic,
                actual_size: _,
                threshold: 1
            })
        );
    }

    #[test]
    fn converting_input_without_timestamp_produces_output_without_timestamp_given_add_timestamp_is_false(
    ) {
        let mut converter = AzureConverter::new(false, Box::new(TestClock), "te");

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
        let mut converter = AzureConverter::new(false, Box::new(TestClock), "te");

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
        let mut converter = AzureConverter::new(true, Box::new(TestClock), "te");

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
        let mut converter = AzureConverter::new(true, Box::new(TestClock), "te");

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

    #[test_case(
        "te/device/main///m/m_type",
        "az/messages/events/",
        r#"{"temperature":23.0,"time":"2021-04-08T00:00:00+05:00"}"#
        ; "main device measurement"
    )]
    #[test_case(
        "te/device/child///m/m_type",
        "az/messages/events/",
        r#"{"temperature":23.0,"time":"2021-04-08T00:00:00+05:00"}"#
        ; "child device measurement"
    )]
    #[test_case(
        "te/device/main/service/m_service/m/m_type",
        "az/messages/events/",
        r#"{"temperature":23.0,"time":"2021-04-08T00:00:00+05:00"}"#
        ; "main device service measurement"
    )]
    #[test_case(
        "te/device/child/service/c_service/m/m_type",
        "az/messages/events/",
        r#"{"temperature":23.0,"time":"2021-04-08T00:00:00+05:00"}"#
        ; "child device service measurement"
    )]
    #[test_case(
        "te/device/main///e/e_type",
        "az/messages/events/",
        r#"{"text":"someone logged-in","time":"2021-04-08T00:00:00+05:00"}"#
        ; "main device event"
    )]
    #[test_case(
        "te/device/child///e/e_type",
        "az/messages/events/",
        r#"{"text":"someone logged-in","time":"2021-04-08T00:00:00+05:00"}"#
        ; "child device event"
    )]
    #[test_case(
        "te/device/main/service/m_service/e/e_type",
        "az/messages/events/",
        r#"{"text":"someone logged-in","time":"2021-04-08T00:00:00+05:00"}"#
        ; "main device service event"
    )]
    #[test_case(
        "te/device/child/service/c_service/e/e_type",
        "az/messages/events/",
        r#"{"text":"someone logged-in","time":"2021-04-08T00:00:00+05:00"}"#
        ; "child device service event"
    )]
    #[test_case(
        "te/device/main///a/a_type",
        "az/messages/events/",
        r#"{"severity":"critical","time":"2021-04-08T00:00:00+05:00"}"#
        ; "main device alarm"
    )]
    #[test_case(
        "te/device/child///a/a_type",
        "az/messages/events/",
        r#"{"severity":"critical","time":"2021-04-08T00:00:00+05:00"}"#
        ; "child device alarm"
    )]
    #[test_case(
        "te/device/main/service/m_service/a/a_type",
        "az/messages/events/",
        r#"{"severity":"critical","time":"2021-04-08T00:00:00+05:00"}"#
        ; "main device service alarm"
    )]
    #[test_case(
        "te/device/child/service/c_service/a/a_type",
        "az/messages/events/",
        r#"{"severity":"critical","time":"2021-04-08T00:00:00+05:00"}"#
        ; "child device service alarm"
    )]
    fn converting_az_telemetry(input_topic: &str, output_topic: &str, input: &str) {
        let mut converter = AzureConverter::new(true, Box::new(TestClock), "te");
        let input_message = MqttMessage::new(&Topic::new_unchecked(input_topic), input);

        let output = converter.convert(&input_message).unwrap();

        assert_eq!(output[0].payload_str().unwrap(), input);
        assert_eq!(output[0].topic.name, output_topic);
    }

    #[test]
    fn converting_bridge_health_status() {
        let mut converter = AzureConverter::new(false, Box::new(TestClock), "te");

        let input = "0";
        let result = converter.try_convert(&MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/mosquitto-az-bridge/status/health"),
            input,
        ));
        let res = result.unwrap();
        assert!(res.is_empty());
    }

    #[test]
    fn converting_service_health_status_up_message() {
        let mut converter = AzureConverter::new(false, Box::new(TestClock), "te");

        let input = r#"{"pid":1234,"status":"up","time":1694586060}"#;
        let result = converter.try_convert(&MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/tedge-mapper-az/status/health"),
            input,
        ));

        let expected_msg = MqttMessage::new(&Topic::new_unchecked("az/messages/events/"), input);
        let res = result.unwrap();
        assert_eq!(res[0], expected_msg);
    }

    #[test]
    fn converting_service_health_status_down_message() {
        let mut converter = AzureConverter::new(false, Box::new(TestClock), "te");

        let input = r#"{"pid":1234,"status":"up"}"#;
        let result = converter.try_convert(&MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/tedge-mapper-az/status/health"),
            input,
        ));

        let expected_msg = MqttMessage::new(&Topic::new_unchecked("az/messages/events/"), input);
        let res = result.unwrap();
        assert_eq!(res[0], expected_msg);
    }
}
