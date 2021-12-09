use crate::error::*;
use crate::size_threshold::SizeThreshold;
use crate::{converter::*, operations::Operations};
use c8y_smartrest::smartrest_serializer::{SmartRestSerializer, SmartRestSetSupportedOperations};
use c8y_translator::json;
use mqtt_client::{Message, Topic};
use std::collections::HashSet;

const SMARTREST_PUBLISH_TOPIC: &str = "c8y/s/us";

pub struct CumulocityConverter {
    pub(crate) size_threshold: SizeThreshold,
    children: HashSet<String>,
    pub(crate) mapper_config: MapperConfig,
}

impl CumulocityConverter {
    pub fn new(size_threshold: SizeThreshold) -> Self {
        let mut topic_fiter = make_valid_topic_filter_or_panic("tedge/measurements");
        let () = topic_fiter
            .add("tedge/measurements/+")
            .expect("invalid topic filter");

        let mapper_config = MapperConfig {
            in_topic_filter: topic_fiter,
            out_topic: make_valid_topic_or_panic("c8y/measurement/measurements/create"),
            errors_topic: make_valid_topic_or_panic("tedge/errors"),
        };

        let children: HashSet<String> = HashSet::new();
        CumulocityConverter {
            size_threshold,
            children,
            mapper_config,
        }
    }
}

impl Converter for CumulocityConverter {
    type Error = ConversionError;

    fn get_mapper_config(&self) -> &MapperConfig {
        &self.mapper_config
    }

    fn try_convert(&mut self, input: &Message) -> Result<Vec<Message>, ConversionError> {
        let () = self.size_threshold.validate(input.payload_str()?)?;

        let mut vec: Vec<Message> = Vec::new();

        let maybe_child_id = get_child_id_from_topic(&input.topic.name)?;
        match maybe_child_id {
            Some(child_id) => {
                // Need to check if the input Thin Edge JSON is valid before adding a child ID to list
                let c8y_json_child_payload =
                    json::from_thin_edge_json_with_child(input.payload_str()?, child_id.as_str())?;

                if !self.children.contains(child_id.as_str()) {
                    self.children.insert(child_id.clone());
                    vec.push(Message::new(
                        &Topic::new_unchecked(SMARTREST_PUBLISH_TOPIC),
                        format!("101,{},{},thin-edge.io-child", child_id, child_id),
                    ));
                }

                vec.push(Message::new(
                    &self.mapper_config.out_topic,
                    c8y_json_child_payload,
                ));
            }
            None => {
                let c8y_json_payload = json::from_thin_edge_json(input.payload_str()?)?;
                vec.push(Message::new(
                    &self.mapper_config.out_topic,
                    c8y_json_payload,
                ));
            }
        }
        Ok(vec)
    }

    fn try_init_messages(&self) -> Result<Vec<Message>, ConversionError> {
        let ops = Operations::try_new("/etc/tedge/operations")?;
        let ops = ops.get_operations_list("c8y");

        let ops_msg = SmartRestSetSupportedOperations::new(&ops);
        let topic = Topic::new_unchecked("c8y/s/us");
        let msg = Message::new(&topic, ops_msg.to_smartrest()?);
        Ok(vec![msg])
    }
}

fn get_child_id_from_topic(topic: &str) -> Result<Option<String>, ConversionError> {
    match topic.strip_prefix("tedge/measurements/").map(String::from) {
        Some(maybe_id) if maybe_id.is_empty() => {
            Err(ConversionError::InvalidChildId { id: maybe_id })
        }
        option => Ok(option),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use test_case::test_case;

    #[test_case("tedge/measurements/test", Some("test".to_string()); "valid child id")]
    #[test_case("tedge/measurements/", None; "returns an error (empty value)")]
    #[test_case("tedge/measurements", None; "invalid child id (parent topic)")]
    #[test_case("foo/bar", None; "invalid child id (invalid topic)")]
    fn extract_child_id(in_topic: &str, expected_child_id: Option<String>) {
        match get_child_id_from_topic(in_topic) {
            Ok(maybe_id) => assert_eq!(maybe_id, expected_child_id),
            Err(ConversionError::InvalidChildId { id }) => {
                assert_eq!(id, "".to_string())
            }
            _ => {
                panic!("Unexpected error type")
            }
        }
    }

    #[test]
    fn convert_thin_edge_json_with_child_id() {
        let mut converter = Box::new(CumulocityConverter::new(SizeThreshold(16 * 1024)));
        let in_topic = "tedge/measurements/child1";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,child1,child1,thin-edge.io-child",
        );
        let expected_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"type":"ThinEdgeMeasurement","externalSource":{"externalId":"child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00"}"#,
        );

        // Test the first output messages contains SmartREST and C8Y JSON.
        let out_first_messages = converter.convert(&in_message);
        assert_eq!(
            out_first_messages,
            vec![
                expected_smart_rest_message,
                expected_c8y_json_message.clone()
            ]
        );

        // Test the second output messages doesn't contain SmartREST child device creation.
        let out_second_messages = converter.convert(&in_message);
        assert_eq!(out_second_messages, vec![expected_c8y_json_message.clone()]);
    }

    #[test]
    fn convert_first_thin_edge_json_invalid_then_valid_with_child_id() {
        let mut converter = Box::new(CumulocityConverter::new(SizeThreshold(16 * 1024)));
        let in_topic = "tedge/measurements/child1";
        let in_invalid_payload = r#"{"temp": invalid}"#;
        let in_valid_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_first_message = Message::new(&Topic::new_unchecked(in_topic), in_invalid_payload);
        let in_second_message = Message::new(&Topic::new_unchecked(in_topic), in_valid_payload);

        // First convert invalid Thin Edge JSON message.
        let out_first_messages = converter.convert(&in_first_message);
        let expected_error_message = Message::new(
            &Topic::new_unchecked("tedge/errors"),
            r#"Invalid JSON: expected value at line 1 column 10: `invalid}`"#,
        );
        assert_eq!(out_first_messages, vec![expected_error_message]);

        // Second convert valid Thin Edge JSON message.
        let out_second_messages = converter.convert(&in_second_message);
        let expected_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,child1,child1,thin-edge.io-child",
        );
        let expected_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"type":"ThinEdgeMeasurement","externalSource":{"externalId":"child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00"}"#,
        );
        assert_eq!(
            out_second_messages,
            vec![
                expected_smart_rest_message,
                expected_c8y_json_message.clone()
            ]
        );
    }

    #[test]
    fn convert_two_thin_edge_json_messages_given_different_child_id() {
        let mut converter = Box::new(CumulocityConverter::new(SizeThreshold(16 * 1024)));
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;

        // First message from "child1"
        let in_first_message = Message::new(
            &Topic::new_unchecked("tedge/measurements/child1"),
            in_payload,
        );
        let out_first_messages = converter.convert(&in_first_message);
        let expected_first_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,child1,child1,thin-edge.io-child",
        );
        let expected_first_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"type":"ThinEdgeMeasurement","externalSource":{"externalId":"child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00"}"#,
        );
        assert_eq!(
            out_first_messages,
            vec![
                expected_first_smart_rest_message,
                expected_first_c8y_json_message
            ]
        );

        // Second message from "child2"
        let in_second_message = Message::new(
            &Topic::new_unchecked("tedge/measurements/child2"),
            in_payload,
        );
        let out_second_messages = converter.convert(&in_second_message);
        let expected_second_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,child2,child2,thin-edge.io-child",
        );
        let expected_second_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"type":"ThinEdgeMeasurement","externalSource":{"externalId":"child2","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00"}"#,
        );
        assert_eq!(
            out_second_messages,
            vec![
                expected_second_smart_rest_message,
                expected_second_c8y_json_message
            ]
        );
    }

    #[test]
    fn check_c8y_threshold_packet_size() -> Result<(), anyhow::Error> {
        let size_threshold = SizeThreshold(16 * 1024);
        let converter = CumulocityConverter::new(size_threshold);
        let buffer = create_packet(1024 * 20);
        let err = converter.size_threshold.validate(&buffer).unwrap_err();
        assert_eq!(
            err.to_string(),
            "The input size 20480 is too big. The threshold is 16384."
        );
        Ok(())
    }

    fn create_packet(size: usize) -> String {
        let data: String = "Some data!".into();
        let loops = size / data.len();
        let mut buffer = String::with_capacity(size);
        for _ in 0..loops {
            buffer.push_str("Some data!");
        }
        buffer
    }
}
