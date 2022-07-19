use crate::c8y::dynamic_discovery::*;
use crate::core::{converter::*, error::*, size_threshold::SizeThreshold};
use agent_interface::{
    topic::{RequestTopic, ResponseTopic},
    Auth, DownloadInfo, Jsonify, OperationStatus, RestartOperationRequest,
    RestartOperationResponse, SoftwareListRequest, SoftwareListResponse, SoftwareUpdateResponse,
};
use async_trait::async_trait;
use c8y_api::{
    http_proxy::C8YHttpProxy,
    json_c8y::{C8yCreateEvent, C8yUpdateSoftwareListResponse},
};
use c8y_smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_smartrest::{
    alarm,
    error::SmartRestDeserializerError,
    operations::{get_operation, Operations},
    smartrest_deserializer::{SmartRestRestartRequest, SmartRestUpdateSoftware},
    smartrest_serializer::{
        CumulocitySupportedOperations, SmartRestGetPendingOperations, SmartRestSerializer,
        SmartRestSetOperationToExecuting, SmartRestSetOperationToFailed,
        SmartRestSetOperationToSuccessful, SmartRestSetSupportedOperations,
    },
};
use c8y_translator::json;

use logged_command::LoggedCommand;
use mqtt_channel::{Message, Topic, TopicFilter};
use plugin_sm::operation_logs::OperationLogs;
use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use tedge_config::{get_tedge_config, ConfigSettingAccessor, LogPathSetting};
use thin_edge_json::{alarm::ThinEdgeAlarm, event::ThinEdgeEvent};
use time::format_description::well_known::Rfc3339;

use tracing::{debug, info, log::error};

use super::{
    error::CumulocityMapperError,
    fragments::{C8yAgentFragment, C8yDeviceDataFragment},
    mapper::CumulocityMapper,
    topic::{C8yTopic, MapperSubscribeTopic},
};

const C8Y_CLOUD: &str = "c8y";
const INVENTORY_FRAGMENTS_FILE_LOCATION: &str = "/etc/tedge/device/inventory.json";
const SUPPORTED_OPERATIONS_DIRECTORY: &str = "/etc/tedge/operations";
const INVENTORY_MANAGED_OBJECTS_TOPIC: &str = "c8y/inventory/managedObjects/update";
const SMARTREST_PUBLISH_TOPIC: &str = "c8y/s/us";
const TEDGE_ALARMS_TOPIC: &str = "tedge/alarms/";
const INTERNAL_ALARMS_TOPIC: &str = "c8y-internal/alarms/";
const TEDGE_EVENTS_TOPIC: &str = "tedge/events/";
const C8Y_JSON_MQTT_EVENTS_TOPIC: &str = "c8y/event/events/create";
const TEDGE_AGENT_LOG_DIR: &str = "tedge/agent";

const CREATE_EVENT_SMARTREST_CODE: u16 = 400;

#[derive(Debug)]
pub struct CumulocityConverter<Proxy>
where
    Proxy: C8YHttpProxy,
{
    pub(crate) size_threshold: SizeThreshold,
    children: HashSet<String>,
    pub(crate) mapper_config: MapperConfig,
    device_name: String,
    device_type: String,
    alarm_converter: AlarmConverter,
    pub operations: Operations,
    operation_logs: OperationLogs,
    http_proxy: Proxy,
}

impl<Proxy> CumulocityConverter<Proxy>
where
    Proxy: C8YHttpProxy,
{
    pub fn new(
        size_threshold: SizeThreshold,
        device_name: String,
        device_type: String,
        operations: Operations,
        http_proxy: Proxy,
    ) -> Result<Self, CumulocityMapperError> {
        let mut topic_filter: TopicFilter = vec![
            "tedge/measurements",
            "tedge/measurements/+",
            "tedge/alarms/+/+",
            "c8y-internal/alarms/+/+",
            "tedge/events/+",
            "tedge/events/+/+",
        ]
        .try_into()
        .expect("topics that mapper should subscribe to");

        let () = topic_filter.add_all(CumulocityMapper::subscriptions(&operations).unwrap());

        let mapper_config = MapperConfig {
            in_topic_filter: topic_filter,
            out_topic: make_valid_topic_or_panic("c8y/measurement/measurements/create"),
            errors_topic: make_valid_topic_or_panic("tedge/errors"),
        };

        let alarm_converter = AlarmConverter::new();

        let children: HashSet<String> = HashSet::new();

        let tedge_config = get_tedge_config()?;
        let logs_path = tedge_config.query(LogPathSetting)?;

        let log_dir = PathBuf::from(&format!("{}/{TEDGE_AGENT_LOG_DIR}", logs_path));

        let operation_logs = OperationLogs::try_new(log_dir)?;

        Ok(CumulocityConverter {
            size_threshold,
            children,
            mapper_config,
            device_name,
            device_type,
            alarm_converter,
            operations,
            operation_logs,
            http_proxy,
        })
    }

    #[cfg(test)]
    pub fn from_logs_path(
        size_threshold: SizeThreshold,
        device_name: String,
        device_type: String,
        operations: Operations,
        http_proxy: Proxy,
        logs_path: PathBuf,
    ) -> Result<Self, CumulocityMapperError> {
        let mut topic_filter: TopicFilter = vec![
            "tedge/measurements",
            "tedge/measurements/+",
            "tedge/alarms/+/+",
            "c8y-internal/alarms/+/+",
            "tedge/events/+",
            "tedge/events/+/+",
        ]
        .try_into()
        .expect("topics that mapper should subscribe to");

        let () = topic_filter.add_all(CumulocityMapper::subscriptions(&operations).unwrap());

        let mapper_config = MapperConfig {
            in_topic_filter: topic_filter,
            out_topic: make_valid_topic_or_panic("c8y/measurement/measurements/create"),
            errors_topic: make_valid_topic_or_panic("tedge/errors"),
        };

        let alarm_converter = AlarmConverter::new();

        let children: HashSet<String> = HashSet::new();

        let log_dir = PathBuf::from(&format!(
            "{}/{TEDGE_AGENT_LOG_DIR}",
            logs_path.to_str().unwrap()
        ));

        let operation_logs = OperationLogs::try_new(log_dir)?;

        Ok(CumulocityConverter {
            size_threshold,
            children,
            mapper_config,
            device_name,
            device_type,
            alarm_converter,
            operations,
            operation_logs,
            http_proxy,
        })
    }

    fn try_convert_measurement(
        &mut self,
        input: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        let mut vec: Vec<Message> = Vec::new();

        let maybe_child_id = get_child_id_from_measurement_topic(&input.topic.name)?;
        let c8y_json_payload = match maybe_child_id {
            Some(child_id) => {
                // Need to check if the input Thin Edge JSON is valid before adding a child ID to list
                let c8y_json_child_payload =
                    json::from_thin_edge_json_with_child(input.payload_str()?, child_id.as_str())?;

                if !self.children.contains(child_id.as_str()) {
                    self.children.insert(child_id.clone());
                    vec.push(Message::new(
                        &Topic::new_unchecked(SMARTREST_PUBLISH_TOPIC),
                        format!("101,{child_id},{child_id},thin-edge.io-child"),
                    ));
                }
                c8y_json_child_payload
            }
            None => json::from_thin_edge_json(input.payload_str()?)?,
        };

        if c8y_json_payload.len() < self.size_threshold.0 {
            vec.push(Message::new(
                &self.mapper_config.out_topic,
                c8y_json_payload,
            ));
        } else {
            return Err(ConversionError::TranslatedSizeExceededThreshold {
                payload: input.payload_str()?[0..50].into(),
                topic: input.topic.name.clone(),
                actual_size: c8y_json_payload.len(),
                threshold: self.size_threshold.0,
            });
        }
        Ok(vec)
    }

    async fn try_convert_event(
        &mut self,
        input: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        let mut vec = Vec::new();
        let c8y_event;

        let tedge_event = ThinEdgeEvent::try_from(&input.topic.name, input.payload_str()?)?;
        let child_id = tedge_event.source.clone();

        Self::create_external_source_if_does_not_exist(self, &tedge_event, &mut vec);

        // Convert the external source event message to Cumulocity JSON message.
        c8y_event = C8yCreateEvent::try_from(tedge_event)?;

        // If the message doesn't contain any fields other than `text` and `time`, convert to SmartREST
        let message = if c8y_event.extras.is_empty() {
            let smartrest_event = Self::serialize_to_smartrest(&c8y_event)?;
            let smartrest_topic = Topic::new_unchecked(SMARTREST_PUBLISH_TOPIC);
            Message::new(&smartrest_topic, smartrest_event)
        } else {
            // If the message contains extra fields other than `text` and `time`, convert to Cumulocity JSON
            let cumulocity_event_json = serde_json::to_string(&c8y_event)?;
            let json_mqtt_topic = Topic::new_unchecked(C8Y_JSON_MQTT_EVENTS_TOPIC);
            Message::new(&json_mqtt_topic, cumulocity_event_json)
        };

        // If the MQTT message size is well within the Cumulocity MQTT size limit, use MQTT to send the mapped event as well
        if input.payload_bytes().len() < self.size_threshold.0 {
            vec.push(message);
            Ok(vec)
        // If the message size is larger than the MQTT size limit, use HTTP to send the mapped event
        } else if let Some(id) = child_id {
            if self.children.contains(&id) {
                let _ = self.http_proxy.send_event(c8y_event).await?;
                Ok(vec![])
            } else {
                Err(ConversionError::ChildDeviceNotRegistered { id: id.to_string() })
            }
        } else {
            // Parent device
            let _ = self.http_proxy.send_event(c8y_event).await?;
            Ok(vec![])
        }
    }

    fn serialize_to_smartrest(c8y_event: &C8yCreateEvent) -> Result<String, ConversionError> {
        Ok(format!(
            "{},{},\"{}\",{}",
            CREATE_EVENT_SMARTREST_CODE,
            c8y_event.event_type,
            c8y_event.text,
            c8y_event.time.format(&Rfc3339)?
        ))
    }

    fn create_external_source_if_does_not_exist(
        &mut self,
        tedge_event: &ThinEdgeEvent,
        vec: &mut Vec<Message>,
    ) {
        if let Some(c_id) = tedge_event.source.clone() {
            // Create the external source if it does not exists
            if !self.children.contains(&c_id) {
                self.children.insert(c_id.clone());
                vec.push(Message::new(
                    &Topic::new_unchecked(SMARTREST_PUBLISH_TOPIC),
                    format!("101,{c_id},{c_id},thin-edge.io-child"),
                ));
            }
        }
    }
}

#[async_trait]
impl<Proxy> Converter for CumulocityConverter<Proxy>
where
    Proxy: C8YHttpProxy,
{
    type Error = ConversionError;

    fn get_mapper_config(&self) -> &MapperConfig {
        &self.mapper_config
    }
    async fn try_convert(&mut self, message: &Message) -> Result<Vec<Message>, ConversionError> {
        match &message.topic {
            topic if topic.name.starts_with("tedge/measurements") => {
                let () = self.size_threshold.validate(message)?;
                self.try_convert_measurement(message)
            }
            topic if topic.name.starts_with("tedge/alarms") => {
                let () = self.size_threshold.validate(message)?;
                self.alarm_converter.try_convert_alarm(message)
            }
            topic if topic.name.starts_with(INTERNAL_ALARMS_TOPIC) => {
                self.alarm_converter.process_internal_alarm(message);
                Ok(vec![])
            }
            topic if topic.name.starts_with(TEDGE_EVENTS_TOPIC) => {
                self.try_convert_event(message).await
            }
            topic => match topic.clone().try_into() {
                Ok(MapperSubscribeTopic::ResponseTopic(ResponseTopic::SoftwareListResponse)) => {
                    debug!("Software list");
                    Ok(validate_and_publish_software_list(
                        message.payload_str()?,
                        &mut self.http_proxy,
                    )
                    .await?)
                }
                Ok(MapperSubscribeTopic::ResponseTopic(ResponseTopic::SoftwareUpdateResponse)) => {
                    debug!("Software update");
                    Ok(
                        publish_operation_status(message.payload_str()?, &mut self.http_proxy)
                            .await?,
                    )
                }
                Ok(MapperSubscribeTopic::ResponseTopic(ResponseTopic::RestartResponse)) => {
                    Ok(publish_restart_operation_status(message.payload_str()?).await?)
                }
                Ok(MapperSubscribeTopic::C8yTopic(_)) => {
                    parse_c8y_topics(
                        message,
                        &self.operations,
                        &mut self.http_proxy,
                        &self.operation_logs,
                    )
                    .await
                }
                _ => Err(ConversionError::UnsupportedTopic(
                    message.topic.name.clone(),
                )),
            },
        }
    }

    fn try_init_messages(&self) -> Result<Vec<Message>, ConversionError> {
        let inventory_fragments_message =
            self.wrap_error(create_inventory_fragments_message(&self.device_name));
        let supported_operations_message =
            self.wrap_error(create_supported_operations_fragments_message());
        let device_data_message = self.wrap_error(create_device_data_fragments(
            &self.device_name,
            &self.device_type,
        ));

        let pending_operations_message = self.wrap_error(create_get_pending_operations_message());
        let software_list_message = self.wrap_error(create_get_software_list_message());

        Ok(vec![
            inventory_fragments_message,
            supported_operations_message,
            device_data_message,
            pending_operations_message,
            software_list_message,
        ])
    }

    fn sync_messages(&mut self) -> Vec<Message> {
        let sync_messages: Vec<Message> = self.alarm_converter.sync();
        self.alarm_converter = AlarmConverter::Synced;
        sync_messages
    }

    fn try_process_operation_update_message(
        &mut self,
        message: &DiscoverOp,
    ) -> Result<Option<Message>, ConversionError> {
        match message.event_type {
            EventType::Add => {
                let ops_dir = message.ops_dir.clone();
                let op_name = message.operation_name.clone();
                let op = get_operation(ops_dir.join(op_name))?;
                self.operations.add_operation(op);
            }
            EventType::Remove => {
                self.operations.remove_operation(&message.operation_name);
            }
        }
        Ok(Some(create_supported_operations_fragments_message()?))
    }
}

async fn parse_c8y_topics(
    message: &Message,
    operations: &Operations,
    http_proxy: &mut impl C8YHttpProxy,
    operation_logs: &OperationLogs,
) -> Result<Vec<Message>, ConversionError> {
    match process_smartrest(
        message.payload_str()?,
        operations,
        http_proxy,
        operation_logs,
    )
    .await
    {
        Err(
            ref err @ CumulocityMapperError::FromSmartRestDeserializer(
                SmartRestDeserializerError::InvalidParameter { ref operation, .. },
            )
            | ref err @ CumulocityMapperError::ExecuteFailed {
                operation_name: ref operation,
                ..
            },
        ) => {
            let topic = C8yTopic::SmartRestResponse.to_topic()?;
            let msg1 = Message::new(&topic, format!("501,{operation}"));
            let msg2 = Message::new(&topic, format!("502,{operation},\"{}\"", &err.to_string()));
            error!("{err}");
            Ok(vec![msg1, msg2])
        }
        Err(err) => {
            error!("{err}");
            Ok(vec![])
        }

        Ok(msgs) => Ok(msgs),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AlarmConverter {
    Syncing {
        pending_alarms_map: HashMap<String, Message>,
        old_alarms_map: HashMap<String, Message>,
    },
    Synced,
}

impl AlarmConverter {
    fn new() -> Self {
        AlarmConverter::Syncing {
            old_alarms_map: HashMap::new(),
            pending_alarms_map: HashMap::new(),
        }
    }

    fn try_convert_alarm(&mut self, input: &Message) -> Result<Vec<Message>, ConversionError> {
        let mut vec: Vec<Message> = Vec::new();

        match self {
            Self::Syncing {
                pending_alarms_map,
                old_alarms_map: _,
            } => {
                let alarm_id = input
                    .topic
                    .name
                    .strip_prefix(TEDGE_ALARMS_TOPIC)
                    .expect("Expected tedge/alarms prefix")
                    .to_string();
                pending_alarms_map.insert(alarm_id, input.clone());
            }
            Self::Synced => {
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
                    Topic::new_unchecked(format!("{INTERNAL_ALARMS_TOPIC}{alarm_id}").as_str());
                let alarm_copy =
                    Message::new(&topic, input.payload_bytes().to_owned()).with_retain();
                vec.push(alarm_copy);
            }
        }

        Ok(vec)
    }

    fn process_internal_alarm(&mut self, input: &Message) {
        match self {
            Self::Syncing {
                pending_alarms_map: _,
                old_alarms_map,
            } => {
                let alarm_id = input
                    .topic
                    .name
                    .strip_prefix(INTERNAL_ALARMS_TOPIC)
                    .expect("Expected c8y-internal/alarms prefix")
                    .to_string();
                old_alarms_map.insert(alarm_id, input.clone());
            }
            Self::Synced => {
                // Ignore
            }
        }
    }

    /// Detect and sync any alarms that were raised/cleared while this mapper process was not running.
    /// For this syncing logic, converter maintains an internal journal of all the alarms processed by this mapper,
    /// which is compared against all the live alarms seen by the mapper on every startup.
    ///
    /// All the live alarms are received from tedge/alarms topic on startup.
    /// Similarly, all the previously processed alarms are received from c8y-internal/alarms topic.
    /// Sync detects the difference between these two sets, which are the missed messages.
    ///
    /// An alarm that is present in c8y-internal/alarms, but not in tedge/alarms topic
    /// is assumed to have been cleared while the mapper process was down.
    /// Similarly, an alarm that is present in tedge/alarms, but not in c8y-internal/alarms topic
    /// is one that was raised while the mapper process was down.
    /// An alarm present in both, if their payload is the same, is one that was already processed before the restart
    /// and hence can be ignored during sync.
    fn sync(&mut self) -> Vec<Message> {
        let mut sync_messages: Vec<Message> = Vec::new();

        match self {
            Self::Syncing {
                pending_alarms_map,
                old_alarms_map,
            } => {
                // Compare the differences between alarms in tedge/alarms topic to the ones in c8y-internal/alarms topic
                old_alarms_map.drain().for_each(|(alarm_id, old_message)| {
                    match pending_alarms_map.entry(alarm_id.clone()) {
                        // If an alarm that is present in c8y-internal/alarms topic is not present in tedge/alarms topic,
                        // it is assumed to have been cleared while the mapper process was down
                        Entry::Vacant(_) => {
                            let topic = Topic::new_unchecked(
                                format!("{TEDGE_ALARMS_TOPIC}{alarm_id}").as_str(),
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

                pending_alarms_map
                    .drain()
                    .for_each(|(_key, message)| sync_messages.push(message));
            }
            Self::Synced => {
                // Ignore
            }
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

fn create_get_software_list_message() -> Result<Message, ConversionError> {
    let request = SoftwareListRequest::default();
    let topic = Topic::new(RequestTopic::SoftwareListRequest.as_str())?;
    let payload = request.to_json().unwrap();
    Ok(Message::new(&topic, payload))
}

fn create_get_pending_operations_message() -> Result<Message, ConversionError> {
    let data = SmartRestGetPendingOperations::default();
    let topic = C8yTopic::SmartRestResponse.to_topic()?;
    let payload = data.to_smartrest()?;
    Ok(Message::new(&topic, payload))
}

fn create_supported_operations_fragments_message() -> Result<Message, ConversionError> {
    let ops = Operations::try_new(SUPPORTED_OPERATIONS_DIRECTORY, C8Y_CLOUD)?;
    let ops = ops.get_operations_list();
    let ops = ops.iter().map(|op| op as &str).collect::<Vec<&str>>();

    let ops_msg = SmartRestSetSupportedOperations::new(&ops);
    let topic = Topic::new_unchecked(SMARTREST_PUBLISH_TOPIC);
    Ok(Message::new(&topic, ops_msg.to_smartrest()?))
}

fn create_inventory_fragments_message(device_name: &str) -> Result<Message, ConversionError> {
    let ops_msg = get_inventory_fragments(INVENTORY_FRAGMENTS_FILE_LOCATION)?;

    let topic = Topic::new_unchecked(&format!("{INVENTORY_MANAGED_OBJECTS_TOPIC}/{device_name}"));
    Ok(Message::new(&topic, ops_msg.to_string()))
}

async fn publish_restart_operation_status(
    json_response: &str,
) -> Result<Vec<Message>, CumulocityMapperError> {
    let response = RestartOperationResponse::from_json(json_response)?;
    let topic = C8yTopic::SmartRestResponse.to_topic()?;

    match response.status() {
        OperationStatus::Executing => {
            let smartrest_set_operation = SmartRestSetOperationToExecuting::new(
                CumulocitySupportedOperations::C8yRestartRequest,
            )
            .to_smartrest()?;

            Ok(vec![Message::new(&topic, smartrest_set_operation)])
        }
        OperationStatus::Successful => {
            let smartrest_set_operation = SmartRestSetOperationToSuccessful::new(
                CumulocitySupportedOperations::C8yRestartRequest,
            )
            .to_smartrest()?;
            Ok(vec![Message::new(&topic, smartrest_set_operation)])
        }
        OperationStatus::Failed => {
            let smartrest_set_operation = SmartRestSetOperationToFailed::new(
                CumulocitySupportedOperations::C8yRestartRequest,
                "Restart Failed".into(),
            )
            .to_smartrest()?;
            Ok(vec![Message::new(&topic, smartrest_set_operation)])
        }
    }
}

async fn publish_operation_status(
    json_response: &str,
    http_proxy: &mut impl C8YHttpProxy,
) -> Result<Vec<Message>, CumulocityMapperError> {
    let response = SoftwareUpdateResponse::from_json(json_response)?;
    let topic = C8yTopic::SmartRestResponse.to_topic()?;
    match response.status() {
        OperationStatus::Executing => {
            let smartrest_set_operation_status =
                SmartRestSetOperationToExecuting::from_thin_edge_json(response)?.to_smartrest()?;
            Ok(vec![Message::new(&topic, smartrest_set_operation_status)])
        }
        OperationStatus::Successful => {
            let smartrest_set_operation =
                SmartRestSetOperationToSuccessful::from_thin_edge_json(response)?.to_smartrest()?;

            validate_and_publish_software_list(json_response, http_proxy).await?;
            Ok(vec![Message::new(&topic, smartrest_set_operation)])
        }
        OperationStatus::Failed => {
            let smartrest_set_operation =
                SmartRestSetOperationToFailed::from_thin_edge_json(response)?.to_smartrest()?;
            validate_and_publish_software_list(json_response, http_proxy).await?;
            Ok(vec![Message::new(&topic, smartrest_set_operation)])
        }
    }
}

async fn validate_and_publish_software_list(
    payload: &str,
    http_proxy: &mut impl C8YHttpProxy,
) -> Result<Vec<Message>, CumulocityMapperError> {
    let response = &SoftwareListResponse::from_json(payload)?;

    match response.status() {
        OperationStatus::Successful => {
            let c8y_software_list: C8yUpdateSoftwareListResponse = response.into();
            http_proxy
                .send_software_list_http(&c8y_software_list)
                .await?;
        }

        OperationStatus::Failed => {
            error!("Received a failed software response: {payload}");
        }

        OperationStatus::Executing => {} // C8Y doesn't expect any message to be published
    }

    Ok(vec![])
}

async fn execute_operation(
    payload: &str,
    command: &str,
    operation_name: &str,
    operation_logs: &OperationLogs,
) -> Result<(), CumulocityMapperError> {
    let command = command.to_owned();
    let payload = payload.to_string();

    let mut logged = LoggedCommand::new(&command);
    logged.arg(&payload);

    let child = logged
        .spawn()
        .map_err(|e| CumulocityMapperError::ExecuteFailed {
            error_message: e.to_string(),
            command: command.to_string(),
            operation_name: operation_name.to_string(),
        });

    let mut log_file = operation_logs
        .new_log_file(plugin_sm::operation_logs::LogKind::Operation(
            operation_name.to_string(),
        ))
        .await?;

    match child {
        Ok(child) => {
            tokio::spawn(async move {
                let logger = log_file.buffer();
                let _result = child.wait_with_output(logger).await.unwrap();
            });
            Ok(())
        }
        Err(err) => Err(err),
    }
}

async fn process_smartrest(
    payload: &str,
    operations: &Operations,
    http_proxy: &mut impl C8YHttpProxy,
    operation_logs: &OperationLogs,
) -> Result<Vec<Message>, CumulocityMapperError> {
    let message_id: &str = &payload[..3];
    match message_id {
        "528" => forward_software_request(payload, http_proxy).await,
        "510" => forward_restart_request(payload),
        template => forward_operation_request(payload, template, operations, operation_logs).await,
    }
}

async fn forward_software_request(
    smartrest: &str,
    http_proxy: &mut impl C8YHttpProxy,
) -> Result<Vec<Message>, CumulocityMapperError> {
    let topic = Topic::new(RequestTopic::SoftwareUpdateRequest.as_str())?;
    let update_software = SmartRestUpdateSoftware::default();
    let mut software_update_request = update_software
        .from_smartrest(smartrest)?
        .to_thin_edge_json()?;

    let token = http_proxy.get_jwt_token().await?;

    software_update_request
        .update_list
        .iter_mut()
        .for_each(|modules| {
            modules.modules.iter_mut().for_each(|module| {
                if let Some(url) = &module.url {
                    if http_proxy.url_is_in_my_tenant_domain(url.url()) {
                        module.url = module.url.as_ref().map(|s| {
                            DownloadInfo::new(&s.url).with_auth(Auth::new_bearer(&token.token()))
                        });
                    } else {
                        module.url = module.url.as_ref().map(|s| DownloadInfo::new(&s.url));
                    }
                }
            });
        });

    Ok(vec![Message::new(
        &topic,
        software_update_request.to_json().unwrap(),
    )])
}

fn forward_restart_request(smartrest: &str) -> Result<Vec<Message>, CumulocityMapperError> {
    let topic = Topic::new(RequestTopic::RestartRequest.as_str())?;
    let _ = SmartRestRestartRequest::from_smartrest(smartrest)?;

    let request = RestartOperationRequest::default();
    Ok(vec![Message::new(&topic, request.to_json()?)])
}

async fn forward_operation_request(
    payload: &str,
    template: &str,
    operations: &Operations,
    operation_logs: &OperationLogs,
) -> Result<Vec<Message>, CumulocityMapperError> {
    match operations.matching_smartrest_template(template) {
        Some(operation) => {
            if let Some(command) = operation.command() {
                execute_operation(payload, command.as_str(), &operation.name, operation_logs)
                    .await?;
            }
            Ok(vec![])
        }
        None => Ok(vec![]),
    }
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
                .ok_or(ConversionError::FromOptionError)?
                .insert(
                    "c8y_Agent".to_string(),
                    json_fragment
                        .get("c8y_Agent")
                        .ok_or(ConversionError::FromOptionError)?
                        .to_owned(),
                );
            Ok(json)
        }
        Err(_) => {
            info!("Inventory fragments file not found at {INVENTORY_FRAGMENTS_FILE_LOCATION}");
            Ok(json_fragment)
        }
    }
}

pub fn get_child_id_from_measurement_topic(topic: &str) -> Result<Option<String>, ConversionError> {
    match topic.strip_prefix("tedge/measurements/").map(String::from) {
        Some(maybe_id) if maybe_id.is_empty() => {
            Err(ConversionError::InvalidChildId { id: maybe_id })
        }
        option => Ok(option),
    }
}

#[cfg(test)]
mod tests {
    use plugin_sm::operation_logs::OperationLogs;
    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn test_execute_operation_is_not_blocked() {
        let log_dir = TempTedgeDir::new();
        let operation_logs = OperationLogs::try_new(log_dir.path().to_path_buf()).unwrap();

        let now = std::time::Instant::now();
        let () = super::execute_operation("5", "sleep", "sleep_one", &operation_logs)
            .await
            .unwrap();
        let () = super::execute_operation("5", "sleep", "sleep_two", &operation_logs)
            .await
            .unwrap();

        // a result between now and elapsed that is not 0 probably means that the operations are
        // blocking and that you probably removed a tokio::spawn handle (;
        assert_eq!(now.elapsed().as_secs(), 0);
    }
}
