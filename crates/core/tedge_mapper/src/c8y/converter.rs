use crate::c8y::dynamic_discovery::*;
use crate::c8y::json;
use crate::core::converter::*;
use crate::core::error::*;
use crate::core::size_threshold::SizeThreshold;
use async_trait::async_trait;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::json_c8y::C8yUpdateSoftwareListResponse;
use c8y_api::smartrest::error::SmartRestDeserializerError;
use c8y_api::smartrest::operations::get_operation;
use c8y_api::smartrest::operations::Operations;
use c8y_api::smartrest::smartrest_deserializer::AvailableChildDevices;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRestartRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestUpdateSoftware;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRestGetPendingOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRestSerializer;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToExecuting;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToFailed;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToSuccessful;
use c8y_api::utils::child_device::new_child_device_message;
use futures::channel::mpsc;
use futures::SinkExt;
use logged_command::LoggedCommand;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use plugin_sm::operation_logs::OperationLogs;
use service_monitor::convert_health_status_message;
use std::collections::HashMap;

use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use tedge_api::event::error::ThinEdgeJsonDeserializerError;
use tedge_api::event::ThinEdgeEvent;
use tedge_api::topic::RequestTopic;
use tedge_api::topic::ResponseTopic;
use tedge_api::Auth;
use tedge_api::DownloadInfo;
use tedge_api::Jsonify;
use tedge_api::OperationStatus;
use tedge_api::RestartOperationRequest;
use tedge_api::RestartOperationResponse;
use tedge_api::SoftwareListRequest;
use tedge_api::SoftwareListResponse;
use tedge_api::SoftwareUpdateResponse;
use tedge_config::get_tedge_config;
use tedge_config::ConfigSettingAccessor;
use tedge_config::LogPathSetting;
use time::format_description::well_known::Rfc3339;
use tracing::debug;
use tracing::info;
use tracing::log::error;

use super::alarm_converter::AlarmConverter;
use super::error::CumulocityMapperError;
use super::fragments::C8yAgentFragment;
use super::fragments::C8yDeviceDataFragment;
use super::service_monitor;
use c8y_api::smartrest::message::collect_smartrest_messages;
use c8y_api::smartrest::message::get_failure_reason_for_smartrest;
use c8y_api::smartrest::message::get_smartrest_device_id;
use c8y_api::smartrest::message::get_smartrest_template_id;
use c8y_api::smartrest::message::sanitize_for_smartrest;
use c8y_api::smartrest::message::MAX_PAYLOAD_LIMIT_IN_BYTES;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_api::smartrest::topic::MapperSubscribeTopic;
use c8y_api::smartrest::topic::SMARTREST_PUBLISH_TOPIC;

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
pub struct CumulocityDeviceInfo {
    pub device_name: String,
    pub device_type: String,
    pub operations: Operations,
}

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
    pub cfg_dir: PathBuf,
    pub children: HashMap<String, Operations>,
    mqtt_publisher: mpsc::UnboundedSender<Message>,
}

impl<Proxy> CumulocityConverter<Proxy>
where
    Proxy: C8YHttpProxy,
{
    pub fn new(
        size_threshold: SizeThreshold,
        device_info: CumulocityDeviceInfo,
        http_proxy: Proxy,
        cfg_dir: &Path,
        children: HashMap<String, Operations>,
        mapper_config: MapperConfig,
        mqtt_publisher: mpsc::UnboundedSender<Message>,
    ) -> Result<Self, CumulocityMapperError> {
        let alarm_converter = AlarmConverter::new();

        let tedge_config = get_tedge_config()?;
        let logs_path = tedge_config.query(LogPathSetting)?;

        let log_dir = PathBuf::from(&format!("{}/{TEDGE_AGENT_LOG_DIR}", logs_path));

        let operation_logs = OperationLogs::try_new(log_dir)?;

        let device_name = device_info.device_name;
        let device_type = device_info.device_type;
        let operations = device_info.operations;

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
            mqtt_publisher,
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
        mapper_config: MapperConfig,
        mqtt_publisher: mpsc::UnboundedSender<Message>,
    ) -> Result<Self, CumulocityMapperError> {
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
            mqtt_publisher,
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
                payload: input.payload_str()?.chars().take(50).collect(),
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
        let mqtt_topic = input.topic.name.clone();
        let mqtt_payload = input.payload_str().map_err(|e| {
            ThinEdgeJsonDeserializerError::FailedToParsePayloadToString {
                topic: mqtt_topic.clone(),
                error: e.to_string(),
            }
        })?;

        let tedge_event = ThinEdgeEvent::try_from(&mqtt_topic, mqtt_payload).map_err(|e| {
            ThinEdgeJsonDeserializerError::FailedToParseJsonPayload {
                topic: mqtt_topic,
                error: e.to_string(),
                payload: mqtt_payload.chars().take(50).collect(),
            }
        })?;
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

    pub async fn process_health_status_message(
        &mut self,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        let mut mqtt_messages: Vec<Message> = Vec::new();

        // When there is some messages to be sent on behalf of a child device,
        // this child device must be declared first, if not done yet
        let topic_split: Vec<&str> = message.topic.name.split('/').collect();
        if topic_split.len() == 4 {
            let child_id = topic_split[2];
            add_external_device_registration_message(
                child_id.to_string(),
                &mut self.children,
                &mut mqtt_messages,
            );
        }

        let mut message = convert_health_status_message(message, self.device_name.clone());
        mqtt_messages.append(&mut message);
        Ok(mqtt_messages)
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
            topic if topic.name.starts_with("tedge/health") => {
                self.process_health_status_message(message).await
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
                        &self.cfg_dir,
                        &mut self.children,
                        &self.mqtt_publisher,
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

        let cloud_child_devices_message = create_request_for_cloud_child_devices();

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
            cloud_child_devices_message,
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

#[allow(clippy::too_many_arguments)]
async fn parse_c8y_topics(
    message: &Message,
    operations: &Operations,
    http_proxy: &mut impl C8YHttpProxy,
    operation_logs: &OperationLogs,
    device_name: &str,
    config_dir: &Path,
    children: &mut HashMap<String, Operations>,
    mqtt_publisher: &mpsc::UnboundedSender<Message>,
) -> Result<Vec<Message>, ConversionError> {
    let mut output: Vec<Message> = Vec::new();
    for smartrest_message in collect_smartrest_messages(message.payload_str()?) {
        match process_smartrest(
            smartrest_message.as_str(),
            operations,
            http_proxy,
            operation_logs,
            device_name,
            config_dir,
            children,
            mqtt_publisher,
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

fn create_request_for_cloud_child_devices() -> Message {
    Message::new(&Topic::new_unchecked("c8y/s/us"), "105")
}

fn add_external_device_registration_message(
    child_id: String,
    children: &mut HashMap<String, Operations>,
    mqtt_messages: &mut Vec<Message>,
) -> bool {
    if !children.contains_key(&child_id) {
        children.insert(child_id.to_string(), Operations::default());
        mqtt_messages.push(new_child_device_message(&child_id));
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
    mqtt_publisher: &mpsc::UnboundedSender<Message>,
) -> Result<(), CumulocityMapperError> {
    let command = command.to_owned();
    let payload = payload.to_string();

    let mut logged = LoggedCommand::new(&command);
    logged.arg(&payload);

    let maybe_child_process = logged
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

    match maybe_child_process {
        Ok(child_process) => {
            let op_name = operation_name.to_string();
            let mut mqtt_publisher = mqtt_publisher.clone();

            tokio::spawn(async move {
                let logger = log_file.buffer();

                // mqtt client publishes executing
                let topic = C8yTopic::SmartRestResponse.to_topic().unwrap();
                let executing_str = format!("501,{op_name}");
                mqtt_publisher
                    .send(Message::new(&topic, executing_str.as_str()))
                    .await
                    .unwrap_or_else(|err| {
                        error!("Failed to publish a message: {executing_str}. Error: {err}")
                    });

                // execute the command and wait until it finishes
                // mqtt client publishes failed or successful depending on the exit code
                if let Ok(output) = child_process.wait_with_output(logger).await {
                    match output.status.code() {
                        Some(0) => {
                            let sanitized_stdout =
                                sanitize_for_smartrest(output.stdout, MAX_PAYLOAD_LIMIT_IN_BYTES);
                            let successful_str = format!("503,{op_name},\"{sanitized_stdout}\"");
                            mqtt_publisher.send(Message::new(&topic, successful_str.as_str())).await
                                    .unwrap_or_else(|err| {
                                        error!("Failed to publish a message: {successful_str}. Error: {err}")
                                    })
                        }
                        _ => {
                            let failure_reason = get_failure_reason_for_smartrest(
                                output.stderr,
                                MAX_PAYLOAD_LIMIT_IN_BYTES,
                            );
                            let failed_str = format!("502,{op_name},\"{failure_reason}\"");
                            mqtt_publisher
                                .send(Message::new(&topic, failed_str.as_str()))
                                .await
                                .unwrap_or_else(|err| {
                                    error!(
                                        "Failed to publish a message: {failed_str}. Error: {err}"
                                    )
                                })
                        }
                    }
                }
            });
            Ok(())
        }
        Err(err) => Err(err),
    }
}

/// Lists all the locally available child devices linked to this parent device.
///
/// The set of all locally available child devices is defined as any directory
/// created under "`config_dir`/operations/c8y" for example "/etc/tedge/operations/c8y"
pub fn get_local_child_devices_list(
    path: &Path,
) -> Result<std::collections::HashSet<String>, CumulocityMapperError> {
    Ok(fs::read_dir(&path)
        .map_err(|_| CumulocityMapperError::ReadDirError {
            dir: PathBuf::from(&path),
        })?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<PathBuf>, _>>()?
        .into_iter()
        .filter(|path| path.is_dir())
        .map(|entry| entry.file_name().unwrap().to_string_lossy().to_string()) // safe unwrap
        .collect::<std::collections::HashSet<String>>())
}

fn register_child_device_supported_operations(
    config_dir: &Path,
    payload: &str,
    children: &mut HashMap<String, Operations>,
) -> Result<Vec<Message>, CumulocityMapperError> {
    let mut messages_vec = vec![];
    // 106 lists the child devices that are linked with the parent device in the
    //     cloud.
    let path_to_child_devices = config_dir
        .join(SUPPORTED_OPERATIONS_DIRECTORY)
        .join(C8Y_CLOUD);

    let cloud_child_devices = AvailableChildDevices::from_smartrest(payload)?;
    let local_child_devices = get_local_child_devices_list(&path_to_child_devices)?;
    // if there are any local child devices that are not included in the
    // `cloud_child_devices` struct, we create them on the cloud, sending a 101
    // message. Then proceed to declare their supported operations.
    let difference: Vec<&String> = local_child_devices
        .difference(&cloud_child_devices.devices)
        .collect();

    for child_id in difference {
        // here we register new child devices, sending the 101 code
        messages_vec.push(new_child_device_message(child_id));
    }
    // loop over all local child devices and update the operations
    for child_id in local_child_devices {
        // update the children cache with the operations supported
        let ops = Operations::try_new(path_to_child_devices.join(&child_id))?;
        children.insert(child_id.clone(), ops.clone());

        let ops_msg = ops.create_smartrest_ops_message()?;
        let topic_str = format!("{SMARTREST_PUBLISH_TOPIC}/{}", child_id);
        let topic = Topic::new_unchecked(&topic_str);
        messages_vec.push(Message::new(&topic, ops_msg));
    }
    Ok(messages_vec)
}

#[allow(clippy::too_many_arguments)]
async fn process_smartrest(
    payload: &str,
    operations: &Operations,
    http_proxy: &mut impl C8YHttpProxy,
    operation_logs: &OperationLogs,
    device_name: &str,
    config_dir: &Path,
    children: &mut HashMap<String, Operations>,
    mqtt_publisher: &mpsc::UnboundedSender<Message>,
) -> Result<Vec<Message>, CumulocityMapperError> {
    match get_smartrest_device_id(payload) {
        Some(device_id) if device_id == device_name => {
            match get_smartrest_template_id(payload).as_str() {
                "528" => forward_software_request(payload, http_proxy).await,
                "510" => forward_restart_request(payload),
                template => {
                    forward_operation_request(
                        payload,
                        template,
                        operations,
                        operation_logs,
                        mqtt_publisher,
                    )
                    .await
                }
            }
        }
        _ => {
            match get_smartrest_template_id(payload).as_str() {
                "106" => register_child_device_supported_operations(config_dir, payload, children),
                // Ignore any other child device incoming request as not yet supported
                _ => {
                    debug!("Ignored. Message not yet supported: {payload}");
                    Ok(vec![])
                }
            }
        }
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
    mqtt_publisher: &mpsc::UnboundedSender<Message>,
) -> Result<Vec<Message>, CumulocityMapperError> {
    if let Some(operation) = operations.matching_smartrest_template(template) {
        if let Some(command) = operation.command() {
            execute_operation(
                payload,
                command.as_str(),
                &operation.name,
                operation_logs,
                mqtt_publisher,
            )
            .await?;
        }
    }
    // MQTT messages will be sent during the operation execution
    Ok(vec![])
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
    info!("Read the fragments from {file_path} file");
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
        Err(ConversionError::FromStdIo(_)) => {
            info!("Could not read inventory fragments from file {inventory_file_path}");
            Ok(json_fragment)
        }
        Err(ConversionError::FromSerdeJson(e)) => {
            info!("Could not parse the {inventory_file_path} file due to: {e}");
            Ok(json_fragment)
        }
        Err(_) => Ok(json_fragment),
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
    use crate::c8y::tests::create_test_mqtt_client_with_empty_operations;
    use crate::c8y::tests::FakeC8YHttpProxy;
    use c8y_api::smartrest::operations::Operations;
    use plugin_sm::operation_logs::OperationLogs;
    use rand::prelude::Distribution;
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    use std::collections::HashMap;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;

    const OPERATIONS: &[&str] = &[
        "c8y_DownloadConfigFile",
        "c8y_LogfileRequest",
        "c8y_SoftwareUpdate",
        "c8y_Command",
    ];

    const EXPECTED_CHILD_DEVICES: &[&str] = &["child-0", "child-1", "child-2", "child-3"];

    #[tokio::test]
    async fn test_execute_operation_is_not_blocked() {
        let log_dir = TempTedgeDir::new();
        let operation_logs = OperationLogs::try_new(log_dir.path().to_path_buf()).unwrap();

        let mqtt_client = create_test_mqtt_client_with_empty_operations().await;

        let now = std::time::Instant::now();
        super::execute_operation(
            "5",
            "sleep",
            "sleep_one",
            &operation_logs,
            &mqtt_client.published,
        )
        .await
        .unwrap();
        super::execute_operation(
            "5",
            "sleep",
            "sleep_two",
            &operation_logs,
            &mqtt_client.published,
        )
        .await
        .unwrap();

        // a result between now and elapsed that is not 0 probably means that the operations are
        // blocking and that you probably removed a tokio::spawn handle (;
        assert_eq!(now.elapsed().as_secs(), 0);
    }

    #[tokio::test]
    async fn ignore_operations_for_child_device() {
        let mqtt_client = create_test_mqtt_client_with_empty_operations().await;
        let output = super::process_smartrest(
            "528,childId,software_a,version_a,url_a,install",
            &Default::default(),
            &mut FakeC8YHttpProxy {},
            &OperationLogs {
                log_dir: Default::default(),
            },
            "testDevice",
            std::path::Path::new(""),
            &mut std::collections::HashMap::default(),
            &mqtt_client.published,
        )
        .await
        .unwrap();
        assert_eq!(output, vec![]);
    }

    /// Creates `n` child devices named "child-n".
    /// for each child device a `k` is selected using a random seed so that
    /// each child devices is assigned a random set of operations.
    ///
    /// The resulting dir structure is the following:
    /// .
    /// └── operations
    ///     └── c8y
    ///         ├── child-0
    ///         │   └── c8y_LogfileRequest
    ///         ├── child-1
    ///         │   ├── c8y_Command
    ///         │   ├── c8y_DownloadConfigFile
    ///         │   └── c8y_SoftwareUpdate
    ///         ├── child-2
    ///         │   ├── c8y_Command
    ///         │   ├── c8y_DownloadConfigFile
    ///         │   └── c8y_LogfileRequest
    ///         └── child-3
    ///             └── c8y_LogfileRequest
    fn make_n_child_devices_with_k_operations(n: u8, ttd: &TempTedgeDir) {
        let mut rng = rand::rngs::StdRng::seed_from_u64(11);
        let dist = rand::distributions::Uniform::from(1..OPERATIONS.len());

        for i in 0..n {
            let dir = ttd.dir(&format!("child-{i}"));

            let k = dist.sample(&mut rng);
            let operations: Vec<_> = OPERATIONS
                .choose_multiple(&mut rand::thread_rng(), k)
                .collect();
            for op in &operations {
                dir.file(op);
            }
        }
    }

    /// Tests that the child device cache is updated and that only devices represented locally are
    /// actually updated to the cloud.
    ///
    /// This means that:
    ///     - Any child device that is not present locally but is seen in the cloud, will not be
    ///     updated with operations. This child device will not be cached.
    ///
    ///     - Any child device that is present locally but not in the cloud will be created and
    ///     then supported operations will be published to the cloud and the device will be cached.
    #[test_case("106", EXPECTED_CHILD_DEVICES; "cloud representation is empty")]
    #[test_case("106,child-one,child-two", EXPECTED_CHILD_DEVICES; "cloud representation is completely different")]
    #[test_case("106,child-3,child-one,child-1", &["child-0", "child-2"]; "cloud representation has some similar child devices")]
    #[test_case("106,child-0,child-1,child-2,child-3", &[]; "cloud representation has seen all child devices")]
    #[tokio::test]
    async fn test_child_device_cache_is_updated(
        cloud_child_devices: &str,
        expected_101_child_devices: &[&str],
    ) {
        let ttd = TempTedgeDir::new();
        let dir = ttd.dir("operations").dir("c8y");
        make_n_child_devices_with_k_operations(4, &dir);
        let mut hm: HashMap<String, Operations> = HashMap::default();
        let mqtt_client = create_test_mqtt_client_with_empty_operations().await;

        let output_messages = super::process_smartrest(
            cloud_child_devices,
            &Default::default(),
            &mut FakeC8YHttpProxy {},
            &OperationLogs {
                log_dir: Default::default(),
            },
            "testDevice",
            ttd.path(),
            &mut hm,
            &mqtt_client.published,
        )
        .await
        .unwrap();

        let mut actual_child_devices: Vec<String> = hm.into_keys().collect();
        actual_child_devices.sort();

        assert_eq!(actual_child_devices, EXPECTED_CHILD_DEVICES);

        let mut supported_operations_counter = 0;
        // Checking `output_messages` for device create 101 events.
        let mut message_hm = HashMap::new();
        for message in output_messages {
            let mut payload = message.payload_str().unwrap().split(',');
            let smartrest_id = payload.next().unwrap().to_string();

            if smartrest_id == "101" {
                let child_id = payload.next().unwrap().to_string();
                let entry = message_hm.entry(child_id).or_insert(vec![]);
                entry.push(smartrest_id.clone());
            }

            if smartrest_id == "114" {
                supported_operations_counter += 1;
            }
        }

        for child in expected_101_child_devices {
            assert!(message_hm.contains_key(*child));
        }

        // no matter what, we expected 114 to happen for all 4 child devices.
        assert_eq!(supported_operations_counter, 4);
    }
}
