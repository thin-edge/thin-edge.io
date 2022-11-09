use crate::c8y::dynamic_discovery::*;
use crate::c8y::json;
use crate::core::{converter::*, error::*, size_threshold::SizeThreshold};
use agent_interface::{
    topic::{RequestTopic, ResponseTopic},
    Auth, DownloadInfo, Jsonify, OperationStatus, RestartOperationRequest,
    RestartOperationResponse, SoftwareListRequest, SoftwareListResponse, SoftwareUpdateResponse,
};
use async_trait::async_trait;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::{
    error::SmartRestDeserializerError,
    operations::{get_operation, Operations},
    smartrest_deserializer::{SmartRestRestartRequest, SmartRestUpdateSoftware},
    smartrest_serializer::{
        CumulocitySupportedOperations, SmartRestGetPendingOperations, SmartRestSerializer,
        SmartRestSetOperationToExecuting, SmartRestSetOperationToFailed,
        SmartRestSetOperationToSuccessful,
    },
};
use c8y_api::{
    http_proxy::C8YHttpProxy,
    json_c8y::{C8yCreateEvent, C8yUpdateSoftwareListResponse},
};
use logged_command::LoggedCommand;
use mqtt_channel::{Message, Topic, TopicFilter};
use plugin_sm::operation_logs::OperationLogs;
use std::collections::HashMap;
use std::fs;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use tedge_config::{get_tedge_config, ConfigSettingAccessor, LogPathSetting};
use thin_edge_json::event::ThinEdgeEvent;
use time::format_description::well_known::Rfc3339;

use tracing::{debug, info, log::error};

use super::alarm_converter::AlarmConverter;
use super::{
    error::CumulocityMapperError,
    fragments::{C8yAgentFragment, C8yDeviceDataFragment},
    mapper::CumulocityMapper,
};
use c8y_api::smartrest::message::{get_smartrest_device_id, get_smartrest_template_id};
use c8y_api::smartrest::topic::{C8yTopic, MapperSubscribeTopic, SMARTREST_PUBLISH_TOPIC};

const C8Y_CLOUD: &str = "c8y";
const INVENTORY_FRAGMENTS_FILE_LOCATION: &str = "device/inventory.json";
const SUPPORTED_OPERATIONS_DIRECTORY: &str = "operations";
const INVENTORY_MANAGED_OBJECTS_TOPIC: &str = "c8y/inventory/managedObjects/update";
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
    pub(crate) mapper_config: MapperConfig,
    device_name: String,
    device_type: String,
    alarm_converter: AlarmConverter,
    pub operations: Operations,
    operation_logs: OperationLogs,
    http_proxy: Proxy,
    cfg_dir: PathBuf,
    pub children: HashMap<String, Operations>,
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
        cfg_dir: &Path,
        children: HashMap<String, Operations>,
    ) -> Result<Self, CumulocityMapperError> {
        let mut topic_filter: TopicFilter = vec![
            "tedge/measurements",
            "tedge/measurements/+",
            "tedge/alarms/+/+",
            "tedge/alarms/+/+/+",
            "c8y-internal/alarms/+/+",
            "c8y-internal/alarms/+/+/+",
            "tedge/events/+",
            "tedge/events/+/+",
        ]
        .try_into()
        .expect("topics that mapper should subscribe to");

        topic_filter.add_all(CumulocityMapper::subscriptions(&operations).unwrap());

        let mapper_config = MapperConfig {
            in_topic_filter: topic_filter,
            out_topic: make_valid_topic_or_panic("c8y/measurement/measurements/create"),
            errors_topic: make_valid_topic_or_panic("tedge/errors"),
        };

        let alarm_converter = AlarmConverter::new();

        let tedge_config = get_tedge_config()?;
        let logs_path = tedge_config.query(LogPathSetting)?;

        let log_dir = PathBuf::from(&format!("{}/{TEDGE_AGENT_LOG_DIR}", logs_path));

        let operation_logs = OperationLogs::try_new(log_dir)?;

        Ok(CumulocityConverter {
            size_threshold,
            mapper_config,
            device_name,
            device_type,
            alarm_converter,
            operations,
            operation_logs,
            http_proxy,
            cfg_dir: cfg_dir.to_path_buf(),
            children,
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
        cfg_dir: PathBuf,
    ) -> Result<Self, CumulocityMapperError> {
        let mut topic_filter: TopicFilter = vec![
            "tedge/measurements",
            "tedge/measurements/+",
            "tedge/alarms/+/+",
            "tedge/alarms/+/+/+",
            "c8y-internal/alarms/+/+",
            "c8y-internal/alarms/+/+/+",
            "tedge/events/+",
            "tedge/events/+/+",
        ]
        .try_into()
        .expect("topics that mapper should subscribe to");

        topic_filter.add_all(CumulocityMapper::subscriptions(&operations).unwrap());

        let mapper_config = MapperConfig {
            in_topic_filter: topic_filter,
            out_topic: make_valid_topic_or_panic("c8y/measurement/measurements/create"),
            errors_topic: make_valid_topic_or_panic("tedge/errors"),
        };

        let alarm_converter = AlarmConverter::new();

        let log_dir = PathBuf::from(&format!(
            "{}/{TEDGE_AGENT_LOG_DIR}",
            logs_path.to_str().unwrap()
        ));

        let operation_logs = OperationLogs::try_new(log_dir)?;
        let children: HashMap<String, Operations> = HashMap::new();

        Ok(CumulocityConverter {
            size_threshold,
            mapper_config,
            device_name,
            device_type,
            alarm_converter,
            operations,
            operation_logs,
            http_proxy,
            cfg_dir,
            children,
        })
    }

    fn try_convert_measurement(
        &mut self,
        input: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        let mut mqtt_messages: Vec<Message> = Vec::new();

        let maybe_child_id = get_child_id_from_measurement_topic(&input.topic.name)?;
        let c8y_json_payload = match maybe_child_id {
            Some(child_id) => {
                // Need to check if the input Thin Edge JSON is valid before adding a child ID to list
                let c8y_json_child_payload =
                    json::from_thin_edge_json_with_child(input.payload_str()?, child_id.as_str())?;
                add_external_device_registration_message(
                    child_id,
                    &mut self.children,
                    &mut mqtt_messages,
                );
                c8y_json_child_payload
            }
            None => json::from_thin_edge_json(input.payload_str()?)?,
        };

        if c8y_json_payload.len() < self.size_threshold.0 {
            mqtt_messages.push(Message::new(
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
        Ok(mqtt_messages)
    }

    async fn try_convert_event(
        &mut self,
        input: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        let mut messages = Vec::new();

        let tedge_event = ThinEdgeEvent::try_from(&input.topic.name, input.payload_str()?)?;
        let child_id = tedge_event.source.clone();
        let need_registration = if let Some(child_id) = child_id.clone() {
            add_external_device_registration_message(child_id, &mut self.children, &mut messages)
        } else {
            false
        };

        let c8y_event = C8yCreateEvent::try_from(tedge_event)?;

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

        if self.can_send_over_mqtt(&message) {
            // The message can be sent via MQTT
            messages.push(message);
        } else if !need_registration {
            // The message must be sent over HTTP
            let _ = self.http_proxy.send_event(c8y_event).await?;
            return Ok(vec![]);
        } else {
            // The message should be sent over HTTP but this cannot be done
            return Err(ConversionError::ChildDeviceNotRegistered {
                id: child_id.unwrap_or_else(|| "".into()),
            });
        }
        Ok(messages)
    }

    pub fn process_alarm_messages(
        &mut self,
        topic: &Topic,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        if topic.name.starts_with("tedge/alarms") {
            let mut mqtt_messages: Vec<Message> = Vec::new();
            self.size_threshold.validate(message)?;
            let mut messages = self.alarm_converter.try_convert_alarm(message)?;
            if !messages.is_empty() {
                // When there is some messages to be sent on behalf of a child device,
                // this child device must be declared first, if not done yet
                let topic_split: Vec<&str> = topic.name.split('/').collect();
                if topic_split.len() == 5 {
                    let child_id = topic_split[4];
                    add_external_device_registration_message(
                        child_id.to_string(),
                        &mut self.children,
                        &mut mqtt_messages,
                    );
                }
            }
            mqtt_messages.append(&mut messages);
            Ok(mqtt_messages)
        } else if topic.name.starts_with(INTERNAL_ALARMS_TOPIC) {
            self.alarm_converter.process_internal_alarm(message);
            Ok(vec![])
        } else {
            Err(ConversionError::UnsupportedTopic(topic.name.clone()))
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

    fn can_send_over_mqtt(&self, message: &Message) -> bool {
        message.payload_bytes().len() < self.size_threshold.0
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
                self.size_threshold.validate(message)?;
                self.try_convert_measurement(message)
            }
            topic
                if topic.name.starts_with("tedge/alarms")
                    | topic.name.starts_with(INTERNAL_ALARMS_TOPIC) =>
            {
                self.process_alarm_messages(topic, message)
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
                        &self.device_name,
                    )
                    .await
                }
                _ => Err(ConversionError::UnsupportedTopic(
                    message.topic.name.clone(),
                )),
            },
        }
    }

    fn try_init_messages(&mut self) -> Result<Vec<Message>, ConversionError> {
        let inventory_fragments_message = self.wrap_error(create_inventory_fragments_message(
            &self.device_name,
            &self.cfg_dir,
        ));

        let supported_operations_message = self.wrap_error(create_supported_operations(
            &self.cfg_dir.join("operations").join("c8y"),
        ));
        let sops =
            create_child_supported_operations_fragments_message(&mut self.children, &self.cfg_dir);
        let mut supported_child_operations_message = self.wrap_errors(sops);

        let device_data_message = self.wrap_error(create_device_data_fragments(
            &self.device_name,
            &self.device_type,
        ));

        let pending_operations_message = self.wrap_error(create_get_pending_operations_message());
        let software_list_message = self.wrap_error(create_get_software_list_message());
        let mut msg = vec![
            inventory_fragments_message,
            supported_operations_message,
            device_data_message,
            pending_operations_message,
            software_list_message,
        ];
        msg.append(&mut supported_child_operations_message);
        Ok(msg)
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
        match message.ops_dir.parent() {
            Some(parent_dir) => {
                if parent_dir.eq(&self.cfg_dir.join("operations").join("c8y")) {
                    // operation for parent
                    add_or_remove_operation(message, &mut self.operations)?;
                    Ok(Some(create_supported_operations(&message.ops_dir)?))
                } else {
                    // operation for child
                    let child_op = self
                        .children
                        .entry(get_child_id(&message.ops_dir)?)
                        .or_insert_with(Operations::default);

                    add_or_remove_operation(message, child_op)?;
                    Ok(Some(create_supported_operations(&message.ops_dir)?))
                }
            }
            None => Ok(None),
        }
    }
}

fn get_child_id(dir_path: &PathBuf) -> Result<String, ConversionError> {
    let dir_ele: Vec<&std::ffi::OsStr> = dir_path.as_path().iter().collect();

    match dir_ele.last() {
        Some(child_id) => {
            let child_id = child_id.to_string_lossy().to_string();
            Ok(child_id)
        }
        None => Err(ConversionError::DirPathComponentError {
            dir: dir_path.to_owned(),
        }),
    }
}
fn add_or_remove_operation(
    message: &DiscoverOp,
    ops: &mut Operations,
) -> Result<(), ConversionError> {
    match message.event_type {
        EventType::Add => {
            let ops_dir = message.ops_dir.clone();
            let op_name = message.operation_name.clone();
            let op = get_operation(ops_dir.join(op_name))?;

            ops.add_operation(op);
        }
        EventType::Remove => {
            ops.remove_operation(&message.operation_name);
        }
    }
    Ok(())
}

async fn parse_c8y_topics(
    message: &Message,
    operations: &Operations,
    http_proxy: &mut impl C8YHttpProxy,
    operation_logs: &OperationLogs,
    device_name: &str,
) -> Result<Vec<Message>, ConversionError> {
    let mut output: Vec<Message> = Vec::new();
    for smartrest_message in message.payload_str()?.split('\n') {
        match process_smartrest(
            smartrest_message,
            operations,
            http_proxy,
            operation_logs,
            device_name,
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
                let msg2 =
                    Message::new(&topic, format!("502,{operation},\"{}\"", &err.to_string()));
                error!("{err}");
                output.extend_from_slice(&[msg1, msg2]);
            }
            Err(err) => {
                error!("{err}");
            }

            Ok(msgs) => output.extend_from_slice(&msgs),
        }
    }
    Ok(output)
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

fn is_child_operation_path(path: &Path) -> bool {
    // a `path` can contains operations for the parent or for the child
    // example paths:
    //  {cfg_dir}/operations/c8y/child_name/
    //  {cfg_dir}/operations/c8y/
    //
    // the difference between an operation for the child or for the parent
    // is the existence of a directory after `operations/c8y` or not.
    match path.file_name() {
        Some(file_name) => !file_name.eq("c8y"),
        None => false,
    }
}

fn create_supported_operations(path: &Path) -> Result<Message, ConversionError> {
    if is_child_operation_path(path) {
        // operations for child
        let child_id = get_child_id(&path.to_path_buf())?;
        let stopic = format!("{SMARTREST_PUBLISH_TOPIC}/{}", child_id);

        Ok(Message::new(
            &Topic::new_unchecked(&stopic),
            Operations::try_new(path)?.create_smartrest_ops_message()?,
        ))
    } else {
        // operations for parent
        Ok(Message::new(
            &Topic::new_unchecked(SMARTREST_PUBLISH_TOPIC),
            Operations::try_new(path)?.create_smartrest_ops_message()?,
        ))
    }
}

fn create_child_supported_operations_fragments_message(
    children: &mut HashMap<String, Operations>,
    cfg_dir: &Path,
) -> Result<Vec<Message>, ConversionError> {
    let mut mqtt_messages = Vec::new();
    let ops_dir = format!("{}/{SUPPORTED_OPERATIONS_DIRECTORY}", cfg_dir.display());
    let path: PathBuf = ops_dir.into();
    let child_entries = fs::read_dir(&path.join(C8Y_CLOUD))
        .map_err(|_| ConversionError::ReadDirError {
            dir: PathBuf::from(&path),
        })?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<PathBuf>, _>>()?
        .into_iter()
        .filter(|path| path.is_dir())
        .collect::<Vec<PathBuf>>();

    for cdir in child_entries {
        supported_ops_and_register_device_message_for_child_device(
            cdir,
            children,
            &mut mqtt_messages,
        )?;
    }
    Ok(mqtt_messages)
}

// Check if the child is already created or not
// If not create the child creation message and then the operations message.
fn supported_ops_and_register_device_message_for_child_device(
    cdir: PathBuf,
    children: &mut HashMap<String, Operations>,
    mqtt_messages: &mut Vec<Message>,
) -> Result<(), ConversionError> {
    let ops = Operations::try_new(cdir.clone())?;
    let ops_msg = ops.create_smartrest_ops_message()?;
    if let Some(id) = cdir.file_name() {
        if let Some(child_id) = id.to_str() {
            add_external_device_registration_message(child_id.to_string(), children, mqtt_messages);
            let topic_str = format!("{SMARTREST_PUBLISH_TOPIC}/{}", child_id);
            let topic = Topic::new_unchecked(&topic_str);
            mqtt_messages.push(Message::new(&topic, ops_msg));
        }
    }
    Ok(())
}

fn add_external_device_registration_message(
    child_id: String,
    children: &mut HashMap<String, Operations>,
    mqtt_messages: &mut Vec<Message>,
) -> bool {
    if !children.contains_key(&child_id) {
        children.insert(child_id.to_string(), Operations::default());
        mqtt_messages.push(Message::new(
            &Topic::new_unchecked(SMARTREST_PUBLISH_TOPIC),
            format!("101,{child_id},{child_id},thin-edge.io-child"),
        ));
        return true;
    }
    false
}

fn create_inventory_fragments_message(
    device_name: &str,
    cfg_dir: &Path,
) -> Result<Message, ConversionError> {
    let inventory_file_path = format!("{}/{INVENTORY_FRAGMENTS_FILE_LOCATION}", cfg_dir.display());
    let ops_msg = get_inventory_fragments(&inventory_file_path)?;

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
    device_name: &str,
) -> Result<Vec<Message>, CumulocityMapperError> {
    match get_smartrest_device_id(payload) {
        Some(device_id) if device_id == device_name => {
            match get_smartrest_template_id(payload).as_str() {
                "528" => forward_software_request(payload, http_proxy).await,
                "510" => forward_restart_request(payload),
                template => {
                    forward_operation_request(payload, template, operations, operation_logs).await
                }
            }
        }
        // Ignore all operations for child devices as not yet supported
        _ => Ok(vec![]),
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
            let topic = C8yTopic::SmartRestResponse.to_topic()?;
            let msg1 = Message::new(&topic, format!("501,{}", operation.name));
            let msg2 = Message::new(&topic, format!("503,{}", operation.name));

            Ok(vec![msg1, msg2])
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
fn get_inventory_fragments(
    inventory_file_path: &str,
) -> Result<serde_json::Value, ConversionError> {
    let agent_fragment = C8yAgentFragment::new()?;
    let json_fragment = agent_fragment.to_json()?;

    match read_json_from_file(inventory_file_path) {
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
            info!("Inventory fragments file not found at {inventory_file_path}");
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
    use crate::c8y::tests::FakeC8YHttpProxy;
    use plugin_sm::operation_logs::OperationLogs;
    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn test_execute_operation_is_not_blocked() {
        let log_dir = TempTedgeDir::new();
        let operation_logs = OperationLogs::try_new(log_dir.path().to_path_buf()).unwrap();

        let now = std::time::Instant::now();
        super::execute_operation("5", "sleep", "sleep_one", &operation_logs)
            .await
            .unwrap();
        super::execute_operation("5", "sleep", "sleep_two", &operation_logs)
            .await
            .unwrap();

        // a result between now and elapsed that is not 0 probably means that the operations are
        // blocking and that you probably removed a tokio::spawn handle (;
        assert_eq!(now.elapsed().as_secs(), 0);
    }

    #[tokio::test]
    async fn ignore_operations_for_child_device() {
        let output = super::process_smartrest(
            "528,childId,software_a,version_a,url_a,install",
            &Default::default(),
            &mut FakeC8YHttpProxy {},
            &OperationLogs {
                log_dir: Default::default(),
            },
            "testDevice",
        )
        .await
        .unwrap();
        assert_eq!(output, vec![]);
    }
}
