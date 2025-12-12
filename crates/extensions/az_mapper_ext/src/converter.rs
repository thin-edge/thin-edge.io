use crate::error::ConversionError;
use crate::size_threshold::SizeThreshold;
use clock::Clock;
use log::error;
use serde_json::Map;
use serde_json::Value;
use std::convert::Infallible;
use tedge_actors::Converter;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::models::timestamp::TimeFormat;
use tedge_config::models::TopicPrefix;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;

#[derive(Debug)]
pub struct MapperConfig {
    pub out_topic: Topic,
    pub errors_topic: Topic,
    pub time_format: TimeFormat,
    pub input_topics: String,
}

pub struct AzureConverter {
    pub(crate) add_timestamp: bool,
    pub(crate) clock: Box<dyn Clock>,
    pub(crate) size_threshold: SizeThreshold,
    pub(crate) mapper_config: MapperConfig,
    pub mqtt_schema: MqttSchema,
}

impl AzureConverter {
    pub fn new(
        add_timestamp: bool,
        clock: Box<dyn Clock>,
        mqtt_schema: MqttSchema,
        time_format: TimeFormat,
        topic_prefix: &TopicPrefix,
        max_payload_size: u32,
        input_topics: String,
    ) -> Self {
        let mapper_config = MapperConfig {
            out_topic: Topic::new_unchecked(&format!("{topic_prefix}/messages/events/")),
            errors_topic: mqtt_schema.error_topic(),
            time_format,
            input_topics,
        };
        let size_threshold = SizeThreshold(max_payload_size as usize);
        AzureConverter {
            add_timestamp,
            clock,
            size_threshold,
            mapper_config,
            mqtt_schema,
        }
    }

    fn try_convert(&mut self, input: &MqttMessage) -> Result<Vec<MqttMessage>, ConversionError> {
        let messages = match self.mqtt_schema.entity_channel_of(&input.topic) {
            Ok((_, channel)) => self.try_convert_te_topics(input, channel),
            Err(_) => Ok(Vec::new()),
        }?;

        for message in &messages {
            self.size_threshold.validate(message)?;
        }

        Ok(messages)
    }

    // Todo: The device-id,telemetry kind (Measurement/event/alarm) and telemetry type from the te topic has to be
    // used to push the telemetry messages on to specific azure topic.
    // For now all the messages will be sent over az/messages/events/ topic as this is the default mqtt topic for
    // sending the telemetry on to the azure iot hub.
    fn try_convert_te_topics(
        &mut self,
        input: &MqttMessage,
        channel: Channel,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        // don't convert mosquitto bridge notification topic
        // https://github.com/thin-edge/thin-edge.io/issues/2236
        if input
            .payload
            .as_str()?
            .parse::<u8>()
            .is_ok_and(|n| n == 0 || n == 1)
            && channel == Channel::Health
        {
            return Ok(vec![]);
        }

        match &channel {
            Channel::Measurement { .. }
            | Channel::Event { .. }
            | Channel::Alarm { .. }
            | Channel::Health => match self.with_timestamp(input) {
                Ok(payload) => {
                    let output = MqttMessage::new(&self.mapper_config.out_topic, payload);
                    Ok(vec![output])
                }
                Err(err) => {
                    error!(
                        "Could not add timestamp to payload for {}: {err}. Skipping",
                        self.mapper_config.out_topic
                    );
                    Ok(vec![])
                }
            },
            _ => Ok(vec![]),
        }
    }

    fn with_timestamp(&mut self, input: &MqttMessage) -> Result<String, ConversionError> {
        let time_format = self.mapper_config.time_format;
        let mut payload: Map<String, Value> = serde_json::from_slice(input.payload.as_bytes())?;

        let time = match payload.remove("time") {
            Some(time) => Some(time_format.reformat_json(time)?),
            None if self.add_timestamp => Some(time_format.to_json(self.clock.now())?),
            None => None,
        };

        if let Some(time) = time {
            payload.insert("time".to_owned(), time);
        }

        Ok(serde_json::to_string(&payload)?)
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

    pub fn builtin_flow(&self) -> String {
        let timestamp_step = if self.add_timestamp {
            format!(
                r#"{{ builtin = "add-timestamp", config = {{ property = "time", format = "{time_format}" }}, reformat = true }},"#,
                time_format = self.mapper_config.time_format,
            )
        } else {
            "".to_string()
        };

        format!(
            r#"
input.mqtt.topics = {input_topics}

steps = [
    {{ builtin = "skip-mosquitto-health-status" }},
    {timestamp_step}
    {{ builtin = "cap-payload-size", config = {{ max_size = {max_size} }} }},
]

output.mqtt.topic = "{output_topic}"
errors.mqtt.topic = "{errors_topic}"
"#,
            input_topics = self.mapper_config.input_topics,
            max_size = self.size_threshold.0,
            output_topic = self.mapper_config.out_topic,
            errors_topic = self.mapper_config.errors_topic,
        )
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
    use tedge_config::tedge_toml::AZ_MQTT_PAYLOAD_LIMIT;
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
    fn try_convert_invalid_json_skips_message() {
        let mut converter = create_test_converter(false);

        let input = "This is not Thin Edge JSON";
        let result = converter.try_convert(&new_tedge_message(input));

        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn try_convert_exceeding_threshold_returns_error() {
        let mut converter = AzureConverter::new(
            false,
            Box::new(TestClock),
            MqttSchema::default(),
            TimeFormat::Rfc3339,
            &TopicPrefix::try_from("az").unwrap(),
            1,
            "".to_string(),
        );

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
        let mut converter = create_test_converter(false);

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
        let mut converter = create_test_converter(false);

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
        let mut converter = create_test_converter(false);

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
    fn converting_input_with_unix_timestamp_produces_output_with_rfc3339_timestamp_given_add_timestamp_is_true(
    ) {
        let mut converter = create_test_converter(true);

        let input = r#"{
            "time" : 1702029646,
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "time" : "2023-12-08T10:00:46Z",
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
    fn converting_input_with_unix_timestamp_preserved() {
        let mut converter = create_test_converter(true);

        let input = r#"{
            "time" : 1702029646,
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "time" : "2023-12-08T10:00:46Z",
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
        let mut converter = create_test_converter(true);

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
        let mut converter = create_test_converter(true);
        let input_message = MqttMessage::new(&Topic::new_unchecked(input_topic), input);

        let output = converter.convert(&input_message).unwrap();

        assert_eq!(output[0].payload_str().unwrap(), input);
        assert_eq!(output[0].topic.name, output_topic);
    }

    #[test]
    fn skip_converting_bridge_health_status() {
        let mut converter = create_test_converter(false);

        let input = "0";
        let result = converter.try_convert(&MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/mosquitto-az-bridge/status/health"),
            input,
        ));
        let res = result.unwrap();
        assert!(res.is_empty());
    }

    #[test]
    fn skip_converting_bridge_health_status_for_different_bridge_topic() {
        let mut converter = create_test_converter(false);

        let input = "0";
        let result = converter.try_convert(&MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/mosquitto-xyz-bridge/status/health"),
            input,
        ));
        let res = result.unwrap();
        assert!(res.is_empty());
    }

    #[test]
    fn converting_service_health_status_up_message() {
        let mut converter = AzureConverter::new(
            false,
            Box::new(TestClock),
            MqttSchema::default(),
            TimeFormat::Unix,
            &TopicPrefix::try_from("az").unwrap(),
            AZ_MQTT_PAYLOAD_LIMIT,
            "".to_string(),
        );

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
        let mut converter = create_test_converter(false);

        let input = r#"{"pid":1234,"status":"up"}"#;
        let result = converter.try_convert(&MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/tedge-mapper-az/status/health"),
            input,
        ));

        let expected_msg = MqttMessage::new(&Topic::new_unchecked("az/messages/events/"), input);
        let res = result.unwrap();
        assert_eq!(res[0], expected_msg);
    }

    fn create_test_converter(add_timestamp: bool) -> AzureConverter {
        AzureConverter::new(
            add_timestamp,
            Box::new(TestClock),
            MqttSchema::default(),
            TimeFormat::Rfc3339,
            &TopicPrefix::try_from("az").unwrap(),
            AZ_MQTT_PAYLOAD_LIMIT,
            "".to_string(),
        )
    }
}
