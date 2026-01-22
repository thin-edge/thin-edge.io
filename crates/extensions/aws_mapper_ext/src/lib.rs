use camino::Utf8Path;
use std::time::SystemTime;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::models::TopicPrefix;
use tedge_flows::ConfigError;
use tedge_flows::ConnectedFlowRegistry;
use tedge_flows::FlowContextHandle;
use tedge_flows::FlowError;
use tedge_flows::FlowRegistryExt;
use tedge_flows::JsonValue;
use tedge_flows::Message;
use tedge_flows::UpdateFlowRegistryError;
use tedge_mqtt_ext::Topic;
use tedge_utils::timestamp::TimeFormat;

pub struct AwsConverter {
    input_topics: String,
    topic_prefix: TopicPrefix,
    errors_topic: Topic,
    size_threshold: usize,
    add_timestamp: bool,
    time_format: TimeFormat,
}

impl AwsConverter {
    pub fn new(
        add_timestamp: bool,
        mqtt_schema: &MqttSchema,
        time_format: TimeFormat,
        topic_prefix: TopicPrefix,
        max_payload_size: u32,
        input_topics: String,
    ) -> Self {
        let errors_topic = mqtt_schema.error_topic();
        let size_threshold = max_payload_size as usize;
        AwsConverter {
            input_topics,
            topic_prefix,
            errors_topic,
            size_threshold,
            add_timestamp,
            time_format,
        }
    }

    pub async fn flow_registry(
        &self,
        flows_dir: impl AsRef<Utf8Path>,
    ) -> Result<ConnectedFlowRegistry, UpdateFlowRegistryError> {
        let mut flows = ConnectedFlowRegistry::new(flows_dir);
        flows.register_builtin(SetAwsTopic::default());
        self.persist_builtin_flow(&mut flows).await?;
        Ok(flows)
    }

    pub(crate) async fn persist_builtin_flow(
        &self,
        flows: &mut ConnectedFlowRegistry,
    ) -> Result<(), UpdateFlowRegistryError> {
        flows
            .persist_builtin_flow("mea", self.builtin_flow().as_str())
            .await
    }

    fn builtin_flow(&self) -> String {
        let timestamp_step = if self.add_timestamp {
            format!(
                r#"{{ builtin = "add-timestamp", config = {{ property = "time", format = "{time_format}", reformat = true }} }},"#,
                time_format = self.time_format,
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
    {{ builtin = "limit-payload-size", config = {{ max_size = {max_size} }} }},
    {{ builtin = "set-aws-topic", config = {{ prefix = "{topic_prefix}" }} }},
]

errors.mqtt.topic = "{errors_topic}"
"#,
            input_topics = self.input_topics,
            topic_prefix = self.topic_prefix,
            max_size = self.size_threshold,
            errors_topic = self.errors_topic,
        )
    }
}

// We need to reduce the number of levels in the topic because AWS IoT only supports topics with 7
// slashes (`/`)
//
// Ref: https://docs.aws.amazon.com/general/latest/gr/iot-core.html -> "Maximum number of slashes in
// topic and topic filter"
fn normalize_source_name(topic: &str) -> String {
    topic
        .split('/')
        .skip(1)
        .take(4)
        .filter(|part| !part.is_empty())
        .collect::<Vec<&str>>()
        .join(":")
}

#[derive(Clone, Default)]
pub struct SetAwsTopic {
    prefix: String,
}

impl tedge_flows::Transformer for SetAwsTopic {
    fn name(&self) -> &str {
        "set-aws-topic"
    }

    fn set_config(&mut self, config: JsonValue) -> Result<(), ConfigError> {
        let prefix = config.string_property("prefix").unwrap_or("aws");
        self.prefix = prefix.to_owned();
        Ok(())
    }

    fn on_message(
        &self,
        _timestamp: SystemTime,
        message: &Message,
        _context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        let topic_prefix = &self.prefix;
        let source = normalize_source_name(&message.topic);
        let topic = match message.topic.split('/').collect::<Vec<_>>()[..] {
            [_, _, _, _, _, "m", telemetry_type] => {
                &format!("{topic_prefix}/td/{source}/m/{telemetry_type}")
            }
            [_, _, _, _, _, "e", telemetry_type] => {
                &format!("{topic_prefix}/td/{source}/e/{telemetry_type}")
            }
            [_, _, _, _, _, "a", telemetry_type] => {
                &format!("{topic_prefix}/td/{source}/a/{telemetry_type}")
            }
            [_, _, _, _, _, "status", "health"] => {
                &format!("{topic_prefix}/td/{source}/status/health")
            }
            _ => return Ok(vec![]),
        };

        Ok(vec![Message::new(topic, message.payload.clone())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_json_diff::*;
    use assert_matches::*;
    use serde_json::json;
    use tedge_config::tedge_toml::AWS_MQTT_PAYLOAD_LIMIT;
    use tedge_flows::ConnectedFlowRegistry;
    use tedge_flows::FlowResult;
    use tedge_flows::MessageProcessor;
    use tedge_flows::SourceTag;
    use tedge_mqtt_ext::MqttMessage;
    use time::macros::datetime;
    static TE_MEA_TOPICS: &str =
        r#"["te/+/+/+/+/m/+", "te/+/+/+/+/e/+", "te/+/+/+/+/a/+", "te/+/+/+/+/status/health"]"#;

    fn new_tedge_message(input: &str) -> MqttMessage {
        MqttMessage::new(&Topic::new_unchecked("te/device/main///m/"), input)
    }

    fn extract_first_message_payload(mut messages: Vec<MqttMessage>) -> String {
        messages.pop().unwrap().payload_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn try_convert_invalid_json_skips_message() {
        let mut converter = create_test_converter(false, None, None).await;

        let input = "This is not Thin Edge JSON";
        let result = converter.try_convert(&new_tedge_message(input)).await;

        assert_eq!(result.unwrap()[0].payload_str().unwrap(), input);
    }

    #[tokio::test]
    async fn try_convert_exceeding_threshold_returns_error() {
        let mut converter = create_test_converter(false, Some(1), None).await;

        let input = r#"{"temperature": 21.3}"#;
        let result = converter.try_convert(&new_tedge_message(input)).await;

        assert_matches!(result, Err(FlowError::UnsupportedMessage(_)));
    }

    #[tokio::test]
    async fn converting_input_without_timestamp_produces_output_without_timestamp_given_add_timestamp_is_false(
    ) {
        let mut converter = create_test_converter(false, None, None).await;

        let input = r#"{
            "temperature": 23.0
         }"#;

        let expected_output = json!({
            "temperature": 23.0
        });

        let output = converter.convert(&new_tedge_message(input)).await.unwrap();

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn converting_input_with_timestamp_produces_output_with_timestamp_given_add_timestamp_is_false(
    ) {
        let mut converter = create_test_converter(false, None, None).await;

        let input = r#"{
            "time" : "2013-06-22T17:03:14.000+02:00",
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "time" : "2013-06-22T17:03:14.000+02:00",
            "temperature": 23.0
        });

        let output = converter.convert(&new_tedge_message(input)).await.unwrap();

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn unix_timestamp_is_converted_to_rfc3339() {
        let mut converter = create_test_converter(true, None, None).await;

        let input = r#"{
            "time" : 1702029646,
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "time" : "2023-12-08T10:00:46Z",
            "temperature": 23.0
        });

        let output = converter.convert(&new_tedge_message(input)).await.unwrap();

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn converting_input_with_timestamp_produces_output_with_timestamp_given_add_timestamp_is_true(
    ) {
        let mut converter = create_test_converter(true, None, None).await;

        let input = r#"{
            "time" : "2013-06-22T17:03:14.000+02:00",
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "time" : "2013-06-22T17:03:14.000+02:00",
            "temperature": 23.0
        });

        let output = converter.convert(&new_tedge_message(input)).await.unwrap();

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn converting_input_without_timestamp_produces_output_with_timestamp_given_add_timestamp_is_true(
    ) {
        let mut converter = create_test_converter(true, None, None).await;

        let input = r#"{
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "temperature": 23.0,
            "time": "2021-04-07T19:00:00Z"
        });

        let output = converter.convert(&new_tedge_message(input)).await.unwrap();
        assert_json_eq!(output[0].topic.name, "aws/td/device:main/m/");

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn converting_input_uses_custom_topic_prefix() {
        let mut converter = create_test_converter(true, None, Some("custom-prefix")).await;

        let input = r#"{
            "temperature": 23.0
        }"#;

        let output = converter.convert(&new_tedge_message(input)).await.unwrap();
        assert_json_eq!(output[0].topic.name, "custom-prefix/td/device:main/m/");
    }

    #[tokio::test]
    async fn converting_input_with_measurement_type() {
        let mut converter = create_test_converter(true, None, None).await;

        let input = r#"{
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "temperature": 23.0,
            "time": "2021-04-07T19:00:00Z"
        });
        let input = MqttMessage::new(&Topic::new_unchecked("te/device/main///m/test_type"), input);
        let output = converter.convert(&input).await.unwrap();
        assert_json_eq!(output[0].topic.name, "aws/td/device:main/m/test_type");

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn converting_input_for_child_device_with_measurement_type() {
        let mut converter = create_test_converter(true, None, None).await;

        let input = r#"{
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "temperature": 23.0,
            "time": "2021-04-07T19:00:00Z"
        });
        let input = MqttMessage::new(
            &Topic::new_unchecked("te/device/child///m/test_type"),
            input,
        );
        let output = converter.convert(&input).await.unwrap();
        assert_json_eq!(output[0].topic.name, "aws/td/device:child/m/test_type");

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn converting_input_for_main_device_service_with_measurement_type() {
        let mut converter = create_test_converter(true, None, None).await;

        let input = r#"{
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "temperature": 23.0,
            "time": "2021-04-07T19:00:00Z"
        });
        let input = MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/main_service/m/test_type"),
            input,
        );
        let output = converter.convert(&input).await.unwrap();
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

    #[tokio::test]
    async fn converting_input_for_child_device_service_with_measurement_type() {
        let mut converter = create_test_converter(true, None, None).await;

        let input = r#"{
            "temperature": 23.0
        }"#;

        let expected_output = json!({
            "temperature": 23.0,
            "time": "2021-04-07T19:00:00Z"
        });
        let input = MqttMessage::new(
            &Topic::new_unchecked("te/device/child/service/child_service/m/test_type"),
            input,
        );
        let output = converter.convert(&input).await.unwrap();
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

    #[tokio::test]
    async fn skip_converting_bridge_health_status() {
        let mut converter = create_test_converter(false, None, None).await;

        let input = "0";
        let result = converter
            .try_convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/mosquitto-aws-bridge/status/health"),
                input,
            ))
            .await;
        let res = result.unwrap();
        assert!(res.is_empty());
    }

    #[tokio::test]
    async fn skip_converting_bridge_health_status_for_different_bridge_topic() {
        let mut converter = create_test_converter(false, None, None).await;

        let input = "0";
        let result = converter
            .try_convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/mosquitto-xyz-bridge/status/health"),
                input,
            ))
            .await;
        let res = result.unwrap();
        assert!(res.is_empty());
    }

    #[tokio::test]
    async fn converting_event_for_main_device() {
        let mut converter = create_test_converter(true, None, None).await;

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
        let output = converter.convert(&input).await.unwrap();
        assert_json_eq!(output[0].topic.name, "aws/td/device:main/e/event_type");

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn converting_event_for_child_device() {
        let mut converter = create_test_converter(true, None, None).await;

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
        let output = converter.convert(&input).await.unwrap();
        assert_json_eq!(output[0].topic.name, "aws/td/device:child/e/event_type");

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn converting_event_for_main_device_service() {
        let mut converter = create_test_converter(true, None, None).await;

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
        let output = converter.convert(&input).await.unwrap();
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

    #[tokio::test]
    async fn converting_event_for_child_device_service() {
        let mut converter = create_test_converter(true, None, None).await;

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
        let output = converter.convert(&input).await.unwrap();
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

    #[tokio::test]
    async fn converting_alarm_for_main_device() {
        let mut converter = create_test_converter(true, None, None).await;

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
        let output = converter.convert(&input).await.unwrap();
        assert_json_eq!(output[0].topic.name, "aws/td/device:main/a/alarm_type");

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn converting_alarm_for_main_service() {
        let mut converter = create_test_converter(true, None, None).await;

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
        let output = converter.convert(&input).await.unwrap();
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

    #[tokio::test]
    async fn converting_alarm_for_child_device() {
        let mut converter = create_test_converter(true, None, None).await;

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
        let output = converter.convert(&input).await.unwrap();
        assert_json_eq!(output[0].topic.name, "aws/td/device:child/a/alarm_type");

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&extract_first_message_payload(output))
                .unwrap(),
            expected_output
        );
    }

    #[tokio::test]
    async fn converting_alarm_for_child_service() {
        let mut converter = create_test_converter(true, None, None).await;

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
        let output = converter.convert(&input).await.unwrap();
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

    #[tokio::test]
    async fn converting_service_health_status_up_message() {
        let mut converter = create_test_converter(false, None, None).await;

        let input = r#"{"pid":1234,"status":"up"}"#;
        let result = converter
            .try_convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/test-service/status/health"),
                input,
            ))
            .await;

        let expected_msg = MqttMessage::new(
            &Topic::new_unchecked("aws/td/device:main:service:test-service/status/health"),
            input,
        );
        let res = result.unwrap();
        assert_eq!(res[0], expected_msg);
    }

    #[tokio::test]
    async fn converting_service_health_status_down_message() {
        let mut converter = create_test_converter(false, None, None).await;

        let input = r#"{"pid":1234,"status":"up"}"#;
        let result = converter
            .try_convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/test-service/status/health"),
                input,
            ))
            .await;

        let expected_msg = MqttMessage::new(
            &Topic::new_unchecked("aws/td/device:main:service:test-service/status/health"),
            input,
        );
        let res = result.unwrap();
        assert_eq!(res[0], expected_msg);
    }

    async fn create_test_converter(
        add_timestamp: bool,
        size_threshold: Option<u32>,
        prefix: Option<&str>,
    ) -> AwsFlows {
        let converter = AwsConverter::new(
            add_timestamp,
            &MqttSchema::default(),
            TimeFormat::Rfc3339,
            TopicPrefix::try_from(prefix.unwrap_or("aws")).unwrap(),
            size_threshold.unwrap_or(AWS_MQTT_PAYLOAD_LIMIT),
            TE_MEA_TOPICS.to_string(),
        );
        let flows_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let flows_path = Utf8Path::from_path(flows_dir.path()).unwrap();
        let flows = converter.flow_registry(flows_path).await.unwrap();
        let mut runtime = MessageProcessor::try_new(flows).await.unwrap();
        runtime.load_all_flows().await;

        AwsFlows {
            runtime,
            _flows_dir: flows_dir,
        }
    }

    struct AwsFlows {
        runtime: MessageProcessor<ConnectedFlowRegistry>,
        _flows_dir: tempfile::TempDir,
    }

    impl AwsFlows {
        async fn convert(&mut self, input: &MqttMessage) -> Result<Vec<MqttMessage>, FlowError> {
            self.try_convert(input).await
        }

        async fn try_convert(
            &mut self,
            input: &MqttMessage,
        ) -> Result<Vec<MqttMessage>, FlowError> {
            let now = SystemTime::from(datetime!(2021-04-07 19:00:00 +00));
            let message = Message::from(input.clone());
            let results = self
                .runtime
                .on_message(now, &SourceTag::Mqtt, &message)
                .await;

            let mut output = vec![];
            for result in results {
                match result {
                    FlowResult::Ok { messages, .. } => output.extend(messages),
                    FlowResult::Err { error, .. } => return Err(error),
                }
            }

            let mut messages = vec![];
            for message in output {
                messages.push(MqttMessage::try_from(message)?);
            }
            Ok(messages)
        }
    }
}
