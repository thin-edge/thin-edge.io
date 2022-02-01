use crate::c8y_fragments::{C8yAgentFragment, C8yDeviceDataFragment};
use crate::error::*;
use crate::size_threshold::SizeThreshold;
use crate::{converter::*, operations::Operations};
use c8y_smartrest::alarm;
use c8y_smartrest::smartrest_serializer::{SmartRestSerializer, SmartRestSetSupportedOperations};
use c8y_translator::json;
use mqtt_channel::{Message, Topic};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use thin_edge_json::alarm::ThinEdgeAlarm;
use tracing::info;

const C8Y_CLOUD: &str = "c8y";
const INVENTORY_FRAGMENTS_FILE_LOCATION: &str = "/etc/tedge/device/inventory.json";
const INVENTORY_MANAGED_OBJECTS_TOPIC: &str = "c8y/inventory/managedObjects/update";
const SUPPORTED_OPERATIONS_DIRECTORY: &str = "/etc/tedge/operations";
const SMARTREST_PUBLISH_TOPIC: &str = "c8y/s/us";
const TEDGE_ALARMS_TOPIC: &str = "tedge/alarms/";
const INTERNAL_ALARMS_TOPIC: &str = "c8y-internal/alarms/";

pub struct CumulocityConverter {
    pub(crate) size_threshold: SizeThreshold,
    children: HashSet<String>,
    pub(crate) mapper_config: MapperConfig,
    device_name: String,
    device_type: String,

    syncing: bool,
    pending_alarms_map: HashMap<String, Message>,
    old_alarms_map: HashMap<String, Message>,
}

impl CumulocityConverter {
    pub fn new(size_threshold: SizeThreshold, device_name: String, device_type: String) -> Self {
        let mut topic_fiter = make_valid_topic_filter_or_panic("tedge/measurements");
        let () = topic_fiter
            .add("tedge/measurements/+")
            .expect("invalid measurement topic filter");
        let () = topic_fiter
            .add("tedge/alarms/+/+")
            .expect("invalid alarm topic filter");
        let () = topic_fiter
            .add("c8y-internal/alarms/+/+")
            .expect("invalid alarm topic filter");

        let mapper_config = MapperConfig {
            in_topic_filter: topic_fiter,
            out_topic: make_valid_topic_or_panic("c8y/measurement/measurements/create"),
            errors_topic: make_valid_topic_or_panic("tedge/errors"),
        };

        let children: HashSet<String> = HashSet::new();

        let pending_alarms_map = HashMap::new();
        let old_alarms_map = HashMap::new();
        let syncing = true;

        CumulocityConverter {
            size_threshold,
            children,
            mapper_config,
            device_name,
            device_type,
            syncing,
            pending_alarms_map,
            old_alarms_map,
        }
    }

    fn try_convert_measurement(
        &mut self,
        input: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
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

    fn try_convert_alarm(&mut self, input: &Message) -> Result<Vec<Message>, ConversionError> {
        let mut vec: Vec<Message> = Vec::new();

        if self.syncing {
            let alarm_id = input
                .topic
                .name
                .strip_prefix(TEDGE_ALARMS_TOPIC)
                .expect("Expected tedge/alarms prefix")
                .to_string();
            self.pending_alarms_map
                .insert(alarm_id.clone(), input.clone());
        } else {
            //Regular conversion phase
            let tedge_alarm =
                ThinEdgeAlarm::try_from(input.topic.name.as_str(), input.payload_str()?)?;
            let smartrest_alarm = alarm::serialize_alarm(tedge_alarm)?;
            let c8y_alarm_topic = Topic::new_unchecked(SMARTREST_PUBLISH_TOPIC);
            vec.push(Message::new(&c8y_alarm_topic, smartrest_alarm));

            // Persist a copy of the alarm to an internal topic for reconciliation on next restart
            let alarm_id = input
                .topic
                .name
                .strip_prefix(TEDGE_ALARMS_TOPIC)
                .expect("Expected tedge/alarms prefix")
                .to_string();
            let topic =
                Topic::new_unchecked(format!("{}{}", INTERNAL_ALARMS_TOPIC, alarm_id).as_str());
            let alarm_copy = Message::new(&topic, input.payload_bytes().to_owned()).with_retain();
            vec.push(alarm_copy);
        }

        Ok(vec)
    }

    fn process_internal_alarm(&mut self, input: &Message) {
        if self.syncing {
            let alarm_id = input
                .topic
                .name
                .strip_prefix(INTERNAL_ALARMS_TOPIC)
                .expect("Expected c8y-internal/alarms prefix")
                .to_string();
            self.old_alarms_map.insert(alarm_id, input.clone());
        } else {
            // Ignore
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
        if input.topic.name.starts_with("tedge/measurement") {
            self.try_convert_measurement(input)
        } else if input.topic.name.starts_with(TEDGE_ALARMS_TOPIC) {
            self.try_convert_alarm(input)
        } else if input.topic.name.starts_with(INTERNAL_ALARMS_TOPIC) {
            self.process_internal_alarm(input);
            Ok(vec![])
        } else {
            Err(ConversionError::UnsupportedTopic(input.topic.name.clone()))
        }
    }

    fn try_init_messages(&self) -> Result<Vec<Message>, ConversionError> {
        let inventory_fragments_message = create_inventory_fragments_message(&self.device_name)?;

        let supported_operations_message = create_supported_operations_fragments()?;

        let device_data_message =
            create_device_data_fragments(&self.device_name, &self.device_type)?;

        Ok(vec![
            supported_operations_message,
            device_data_message,
            inventory_fragments_message,
        ])
    }

    /// Detect and sync any alarms that were raised/cleared while this mapper process was not running.
    /// Reconciliation is done by maintaining an internal journal of all the alarms processed by this mapper,
    /// which is compared against all the live alarms seen by the mapper on every startup.
    ///
    /// All the live alarms are received from tedge/alarms topic on startup.
    /// Similarly, all the previously processed alarms are received from c8y-internal/alarms topic.
    /// Reconiciliation detects the difference between these two sets, which are the missed messages.
    ///
    /// An alarm that is present in c8y-internal/alarms, but not in tedge/alarms topic
    /// is assumed to have been cleared while the mapper process was down.
    /// Similarly, an alarm that is present in tedge/alarms, but not in c8y-internal/alarms topic
    /// is one that was raised while the mapper process was down.
    /// An alarm present in both, if their payload is the same, is one that was already processed before the restart
    /// and hence can be ignored during reconcicilation.
    fn sync_messages(&mut self) -> Vec<Message> {
        self.syncing = false;
        let mut sync_messages: Vec<Message> = Vec::new();

        // Compare the differences between alarms in tedge/alarms topic to the ones in c8y-internal/alarms topic
        self.old_alarms_map
            .drain()
            .for_each(|(alarm_id, old_message)| {
                match self.pending_alarms_map.entry(alarm_id.clone()) {
                    // If an alarm that is present in c8y-internal/alarms topic is not present in tedge/alarms topic,
                    // it is assumed to have been cleared while the mapper process was down
                    Entry::Vacant(_) => {
                        let topic = Topic::new_unchecked(
                            format!("{}{}", TEDGE_ALARMS_TOPIC, alarm_id).as_str(),
                        );
                        let message = Message::new(&topic, vec![]).with_retain();
                        // Recreate the clear alarm message and add it to the pending alarms list to be processed later
                        sync_messages.push(message);
                    }

                    // If the payload of a message received from tedge/alarms is same as one received from c8y-internal/alarms,
                    // it is assumed to be one that was already processed earlier and hence removed from the pending alarms list.
                    Entry::Occupied(entry) => {
                        if entry.get().payload_bytes() == old_message.payload_bytes() {
                            entry.remove();
                        }
                    }
                }
            });

        // Once all the pending alarms are identified, process them
        for (_key, message) in self.pending_alarms_map.drain() {
            sync_messages.push(message);
        }

        sync_messages
    }
}

fn create_device_data_fragments(
    device_name: &str,
    device_type: &str,
) -> Result<Message, ConversionError> {
    let device_data = C8yDeviceDataFragment::from_type(device_type)?;
    let ops_msg = device_data.to_json()?;

    let topic = Topic::new_unchecked(&format!("{INVENTORY_MANAGED_OBJECTS_TOPIC}/{device_name}",));
    Ok(Message::new(&topic, ops_msg.to_string()))
}

fn create_supported_operations_fragments() -> Result<Message, ConversionError> {
    let ops = Operations::try_new(SUPPORTED_OPERATIONS_DIRECTORY, C8Y_CLOUD)?;
    let ops = ops.get_operations_list();
    let ops = ops.iter().map(|op| op as &str).collect::<Vec<&str>>();

    let ops_msg = SmartRestSetSupportedOperations::new(&ops);
    let topic = Topic::new_unchecked(SMARTREST_PUBLISH_TOPIC);
    Ok(Message::new(&topic, ops_msg.to_smartrest()?))
}

fn create_inventory_fragments_message(device_name: &str) -> Result<Message, ConversionError> {
    let ops_msg = get_inventory_fragments(INVENTORY_FRAGMENTS_FILE_LOCATION)?;

    let topic = Topic::new_unchecked(&format!("{INVENTORY_MANAGED_OBJECTS_TOPIC}/{device_name}",));
    Ok(Message::new(&topic, ops_msg.to_string()))
}

/// reads a json file to serde_json::Value
///
/// # Example
/// ```
/// let json_value = read_json_from_file("/path/to/a/file").unwrap();
/// ```
fn read_json_from_file(file_path: &str) -> Result<serde_json::Value, ConversionError> {
    let mut file = File::open(Path::new(file_path))?;
    let mut data = String::new();
    file.read_to_string(&mut data)?;
    let json: serde_json::Value = serde_json::from_str(&data)?;
    Ok(json)
}

/// gets a serde_json::Value of inventory
fn get_inventory_fragments(file_path: &str) -> Result<serde_json::Value, ConversionError> {
    let agent_fragment = C8yAgentFragment::new()?;
    let json_fragment = agent_fragment.to_json()?;

    match read_json_from_file(file_path) {
        Ok(mut json) => {
            json.as_object_mut()
                .ok_or_else(|| return ConversionError::FromOptionError)?
                .insert(
                    "c8y_Agent".to_string(),
                    json_fragment
                        .get("c8y_Agent")
                        .ok_or_else(|| return ConversionError::FromOptionError)?
                        .to_owned(),
                );
            Ok(json)
        }
        Err(_) => {
            info!(
                "Inventory fragments file not found at {}",
                INVENTORY_FRAGMENTS_FILE_LOCATION
            );
            Ok(json_fragment)
        }
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

    use crate::c8y_converter::CumulocityConverter;
    use serde_json::json;
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
        let device_name = String::from("test");
        let device_type = String::from("test_type");

        let mut converter = Box::new(CumulocityConverter::new(
            SizeThreshold(16 * 1024),
            device_name,
            device_type,
        ));
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
        let device_name = String::from("test");
        let device_type = String::from("test_type");

        let mut converter = Box::new(CumulocityConverter::new(
            SizeThreshold(16 * 1024),
            device_name,
            device_type,
        ));
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
        let device_name = String::from("test");
        let device_type = String::from("test_type");

        let mut converter = Box::new(CumulocityConverter::new(
            SizeThreshold(16 * 1024),
            device_name,
            device_type,
        ));
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
        let device_name = String::from("test");
        let device_type = String::from("test_type");

        let converter = Box::new(CumulocityConverter::new(
            SizeThreshold(16 * 1024),
            device_name,
            device_type,
        ));
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

    #[test]
    fn test_sync_alarms() {
        let size_threshold = SizeThreshold(16 * 1024);
        let device_name = String::from("test");

        let mut converter = CumulocityConverter::new(size_threshold, device_name);

        let alarm_topic = "tedge/alarms/critical/temperature_alarm";
        let alarm_payload = r#"{ "message": "Temperature very high" }"#;
        let alarm_message = Message::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

        // During the sync phase, alarms are not converted immediately, but only cached to be synced later
        assert!(converter.convert(&alarm_message).is_empty());

        let non_alarm_topic = "tedge/measurements";
        let non_alarm_payload = r#"{"temp": 1}"#;
        let non_alarm_message =
            Message::new(&Topic::new_unchecked(non_alarm_topic), non_alarm_payload);

        // But non-alarms are converted immediately, even during the sync phase
        assert!(!converter.convert(&non_alarm_message).is_empty());

        // When sync phase is complete, all pending alarms are returned
        let sync_messages = converter.sync_messages();
        assert_eq!(sync_messages.len(), 1);
        let alarm_message = sync_messages.get(0).unwrap();
        assert_eq!(alarm_message.topic.name, alarm_topic);

        // After the sync phase, the conversion of both non-alarms as well as alarms are done immediately
        assert!(!converter.convert(&alarm_message).is_empty());
        assert!(!converter.convert(&non_alarm_message).is_empty());
    }
}
