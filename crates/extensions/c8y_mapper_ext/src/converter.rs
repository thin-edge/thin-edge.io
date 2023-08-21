use super::alarm_converter::AlarmConverter;
use super::config::C8yMapperConfig;
use super::config::MQTT_MESSAGE_SIZE_THRESHOLD;
use super::error::CumulocityMapperError;
use super::fragments::C8yAgentFragment;
use super::fragments::C8yDeviceDataFragment;
use super::service_monitor;
use crate::dynamic_discovery::DiscoverOp;
use crate::error::ConversionError;
use crate::json;
use async_trait::async_trait;
use c8y_api::http_proxy::C8yEndPoint;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::json_c8y::C8yUpdateSoftwareListResponse;
use c8y_api::smartrest::error::OperationsError;
use c8y_api::smartrest::error::SmartRestDeserializerError;
use c8y_api::smartrest::message::collect_smartrest_messages;
use c8y_api::smartrest::message::get_failure_reason_for_smartrest;
use c8y_api::smartrest::message::get_smartrest_device_id;
use c8y_api::smartrest::message::get_smartrest_template_id;
use c8y_api::smartrest::message::sanitize_for_smartrest;
use c8y_api::smartrest::message::MAX_PAYLOAD_LIMIT_IN_BYTES;
use c8y_api::smartrest::operations::get_child_ops;
use c8y_api::smartrest::operations::get_operations;
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
use c8y_api::smartrest::topic::C8yTopic;
use c8y_api::smartrest::topic::MapperSubscribeTopic;
use c8y_api::smartrest::topic::SMARTREST_PUBLISH_TOPIC;
use c8y_api::utils::child_device::new_child_device_message;
use c8y_http_proxy::handle::C8YHttpProxy;
use logged_command::LoggedCommand;
use plugin_sm::operation_logs::OperationLogs;
use plugin_sm::operation_logs::OperationLogsError;
use serde::Deserialize;
use serde::Serialize;
use service_monitor::convert_health_status_message;
use std::collections::HashMap;
use std::fmt::Display;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use tedge_actors::LoggingSender;
use tedge_actors::Sender;
use tedge_api::event::error::ThinEdgeJsonDeserializerError;
use tedge_api::event::ThinEdgeEvent;
use tedge_api::topic::get_child_id_from_measurement_topic;
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
use tedge_config::TEdgeConfigError;
use tedge_mqtt_ext::Message;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_utils::file::create_file_with_defaults;
use tedge_utils::size_threshold::SizeThreshold;
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use tokio::time::Duration;
use tracing::debug;
use tracing::info;
use tracing::log::error;

const C8Y_CLOUD: &str = "c8y";
const INVENTORY_FRAGMENTS_FILE_LOCATION: &str = "device/inventory.json";
const SUPPORTED_OPERATIONS_DIRECTORY: &str = "operations";
const INVENTORY_MANAGED_OBJECTS_TOPIC: &str = "c8y/inventory/managedObjects/update";
const INTERNAL_ALARMS_TOPIC: &str = "c8y-internal/alarms/";
const TEDGE_EVENTS_TOPIC: &str = "tedge/events/";
const C8Y_JSON_MQTT_EVENTS_TOPIC: &str = "c8y/event/events/create";
const TEDGE_AGENT_LOG_DIR: &str = "tedge/agent";
const CREATE_EVENT_SMARTREST_CODE: u16 = 400;
const TEDGE_AGENT_HEALTH_TOPIC: &str = "tedge/health/tedge-agent";

#[derive(Debug)]
pub struct MapperConfig {
    pub out_topic: Topic,
    pub errors_topic: Topic,
}

#[async_trait]
pub trait Converter: Send + Sync {
    type Error: Display;

    fn get_mapper_config(&self) -> &MapperConfig;

    async fn try_convert(&mut self, input: &Message) -> Result<Vec<Message>, Self::Error>;

    async fn convert(&mut self, input: &Message) -> Vec<Message> {
        let messages_or_err = self.try_convert(input).await;
        self.wrap_errors(messages_or_err)
    }

    fn wrap_errors(&self, messages_or_err: Result<Vec<Message>, Self::Error>) -> Vec<Message> {
        messages_or_err.unwrap_or_else(|error| vec![self.new_error_message(error)])
    }

    fn wrap_error(&self, message_or_err: Result<Message, Self::Error>) -> Message {
        message_or_err.unwrap_or_else(|error| self.new_error_message(error))
    }

    fn new_error_message(&self, error: Self::Error) -> Message {
        error!("Mapping error: {}", error);
        Message::new(&self.get_mapper_config().errors_topic, error.to_string())
    }

    fn try_init_messages(&mut self) -> Result<Vec<Message>, Self::Error> {
        Ok(vec![])
    }

    /// This function will be the first method that's called on the converter after it's instantiated.
    /// Return any initialization messages that must be processed before the converter starts converting regular messages.
    fn init_messages(&mut self) -> Vec<Message> {
        match self.try_init_messages() {
            Ok(messages) => messages,
            Err(error) => {
                error!("Mapping error: {}", error);
                vec![Message::new(
                    &self.get_mapper_config().errors_topic,
                    error.to_string(),
                )]
            }
        }
    }

    /// This function will be the called after a brief period(sync window) after the converter starts converting messages.
    /// This gives the converter an opportunity to process the messages received during the sync window and
    /// produce any additional messages as "sync messages" as a result of this processing.
    /// These sync messages will be processed by the mapper right after the sync window before it starts converting further messages.
    /// Typically used to do some processing on all messages received on mapper startup and derive additional messages out of those.
    fn sync_messages(&mut self) -> Vec<Message> {
        vec![]
    }

    fn try_process_operation_update_message(
        &mut self,
        _input: &DiscoverOp,
    ) -> Result<Option<Message>, Self::Error> {
        Ok(None)
    }

    fn process_operation_update_message(&mut self, message: DiscoverOp) -> Message {
        let message_or_err = self.try_process_operation_update_message(&message);
        match message_or_err {
            Ok(Some(msg)) => msg,
            Ok(None) => Message::new(
                &self.get_mapper_config().errors_topic,
                "No operation update required",
            ),
            Err(err) => self.new_error_message(err),
        }
    }
}

pub struct CumulocityConverter {
    pub(crate) size_threshold: SizeThreshold,
    pub(crate) mapper_config: MapperConfig,
    device_name: String,
    device_type: String,
    alarm_converter: AlarmConverter,
    pub operations: Operations,
    operation_logs: OperationLogs,
    mqtt_publisher: LoggingSender<MqttMessage>,
    http_proxy: C8YHttpProxy,
    pub cfg_dir: PathBuf,
    pub ops_dir: PathBuf,
    pub children: HashMap<String, Operations>,
    pub service_type: String,
    pub c8y_endpoint: C8yEndPoint,
}

impl CumulocityConverter {
    pub fn new(
        config: C8yMapperConfig,
        mqtt_publisher: LoggingSender<MqttMessage>,
        http_proxy: C8YHttpProxy,
    ) -> Result<Self, CumulocityConverterBuildError> {
        let device_name = config.device_id.clone();
        let device_type = config.device_type.clone();
        let service_type = config.service_type.clone();
        let c8y_host = config.c8y_host.clone();

        let size_threshold = SizeThreshold(MQTT_MESSAGE_SIZE_THRESHOLD);

        let ops_dir = config.ops_dir;
        let operations = Operations::try_new(ops_dir.clone())?;
        let children = get_child_ops(ops_dir.clone())?;

        let alarm_converter = AlarmConverter::new();

        let log_dir = config.logs_path.join(TEDGE_AGENT_LOG_DIR);
        let operation_logs = OperationLogs::try_new(log_dir.into())?;

        let c8y_endpoint = C8yEndPoint::new(&c8y_host, &device_name);

        let mapper_config = MapperConfig {
            out_topic: Topic::new_unchecked("c8y/measurement/measurements/create"),
            errors_topic: Topic::new_unchecked("tedge/errors"),
        };

        Ok(CumulocityConverter {
            size_threshold,
            mapper_config,
            device_name,
            device_type,
            alarm_converter,
            operations,
            operation_logs,
            http_proxy,
            cfg_dir: config.config_dir,
            ops_dir,
            children,
            mqtt_publisher,
            service_type,
            c8y_endpoint,
        })
    }

    fn try_convert_measurement(
        &mut self,
        input: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        let mut mqtt_messages: Vec<Message> = Vec::new();

        let maybe_child_id = get_child_id_from_measurement_topic(&input.topic.name);
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

        // Send the init messages
        if check_tedge_agent_status(message)? {
            create_tedge_agent_supported_ops(self.ops_dir.clone()).await?;
            mqtt_messages.push(create_get_software_list_message()?);
        }

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

        let mut message = convert_health_status_message(
            message,
            self.device_name.clone(),
            self.service_type.clone(),
        );

        mqtt_messages.append(&mut message);
        Ok(mqtt_messages)
    }

    async fn parse_c8y_topics(
        &mut self,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        let mut output: Vec<Message> = Vec::new();
        for smartrest_message in collect_smartrest_messages(message.payload_str()?) {
            match self.process_smartrest(smartrest_message.as_str()).await {
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

    async fn process_smartrest(
        &mut self,
        payload: &str,
    ) -> Result<Vec<Message>, CumulocityMapperError> {
        match get_smartrest_device_id(payload) {
            Some(device_id) if device_id == self.device_name => {
                match get_smartrest_template_id(payload).as_str() {
                    "528" => self.forward_software_request(payload).await,
                    "510" => Self::forward_restart_request(payload),
                    template => self.forward_operation_request(payload, template).await,
                }
            }
            _ => {
                match get_smartrest_template_id(payload).as_str() {
                    "106" => self.register_child_device_supported_operations(payload),
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
        &mut self,
        smartrest: &str,
    ) -> Result<Vec<Message>, CumulocityMapperError> {
        let topic = Topic::new(RequestTopic::SoftwareUpdateRequest.as_str())?;
        let update_software = SmartRestUpdateSoftware::default();
        let mut software_update_request = update_software
            .from_smartrest(smartrest)?
            .to_thin_edge_json()?;

        // Pass the fresh token to the tedge-agent as it cannot request a new one
        let token = self.http_proxy.get_fresh_jwt_token().await?;

        software_update_request
            .update_list
            .iter_mut()
            .for_each(|modules| {
                modules.modules.iter_mut().for_each(|module| {
                    if let Some(url) = &module.url {
                        if self.c8y_endpoint.url_is_in_my_tenant_domain(url.url()) {
                            module.url = module.url.as_ref().map(|s| {
                                DownloadInfo::new(&s.url).with_auth(Auth::new_bearer(&token))
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
        &mut self,
        payload: &str,
        template: &str,
    ) -> Result<Vec<Message>, CumulocityMapperError> {
        if let Some(operation) = self.operations.matching_smartrest_template(template) {
            if let Some(command) = operation.command() {
                self.execute_operation(
                    payload,
                    command.as_str(),
                    &operation.name,
                    operation.graceful_timeout(),
                    operation.forceful_timeout(),
                )
                .await?;
            }
        }
        // MQTT messages will be sent during the operation execution
        Ok(vec![])
    }

    async fn execute_operation(
        &self,
        payload: &str,
        command: &str,
        operation_name: &str,
        graceful_timeout: Duration,
        forceful_timeout: Duration,
    ) -> Result<(), CumulocityMapperError> {
        let command = command.to_owned();
        let payload = payload.to_string();

        let mut logged = LoggedCommand::new(&command);
        logged.arg(&payload);

        let maybe_child_process =
            logged
                .spawn()
                .map_err(|e| CumulocityMapperError::ExecuteFailed {
                    error_message: e.to_string(),
                    command: command.to_string(),
                    operation_name: operation_name.to_string(),
                });

        let mut log_file = self
            .operation_logs
            .new_log_file(plugin_sm::operation_logs::LogKind::Operation(
                operation_name.to_string(),
            ))
            .await?;

        match maybe_child_process {
            Ok(child_process) => {
                let op_name = operation_name.to_string();
                let mut mqtt_publisher = self.mqtt_publisher.clone();

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
                    if let Ok(output) = child_process
                        .wait_for_output_with_timeout(logger, graceful_timeout, forceful_timeout)
                        .await
                    {
                        match output.status.code() {
                            Some(0) => {
                                let sanitized_stdout = sanitize_for_smartrest(
                                    output.stdout,
                                    MAX_PAYLOAD_LIMIT_IN_BYTES,
                                );
                                let successful_str =
                                    format!("503,{op_name},\"{sanitized_stdout}\"");
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

    fn register_child_device_supported_operations(
        &mut self,
        payload: &str,
    ) -> Result<Vec<Message>, CumulocityMapperError> {
        let mut messages_vec = vec![];
        // 106 lists the child devices that are linked with the parent device in the
        //     cloud.
        let path_to_child_devices = self
            .cfg_dir
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
            self.children.insert(child_id.clone(), ops.clone());

            let ops_msg = ops.create_smartrest_ops_message()?;
            let topic_str = format!("{SMARTREST_PUBLISH_TOPIC}/{}", child_id);
            let topic = Topic::new_unchecked(&topic_str);
            messages_vec.push(Message::new(&topic, ops_msg));
        }
        Ok(messages_vec)
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

#[derive(Error, Debug)]
pub enum CumulocityConverterBuildError {
    #[error(transparent)]
    InvalidConfig(#[from] TEdgeConfigError),

    #[error(transparent)]
    OperationsError(#[from] OperationsError),

    #[error(transparent)]
    OperationLogsError(#[from] OperationLogsError),
}

#[async_trait]
impl Converter for CumulocityConverter {
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
                        self.device_name.clone(), //derive from topic, when supported for child device also.
                    )
                    .await?)
                }
                Ok(MapperSubscribeTopic::ResponseTopic(ResponseTopic::SoftwareUpdateResponse)) => {
                    debug!("Software update");
                    Ok(publish_operation_status(
                        message.payload_str()?,
                        &mut self.http_proxy,
                        self.device_name.clone(),
                    )
                    .await?)
                }
                Ok(MapperSubscribeTopic::ResponseTopic(ResponseTopic::RestartResponse)) => {
                    Ok(publish_restart_operation_status(message.payload_str()?).await?)
                }
                Ok(MapperSubscribeTopic::C8yTopic(_)) => self.parse_c8y_topics(message).await,
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

        Ok(vec![
            inventory_fragments_message,
            supported_operations_message,
            device_data_message,
            pending_operations_message,
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
        // operation for parent
        if message
            .ops_dir
            .eq(&self.cfg_dir.join("operations").join("c8y"))
        {
            // Re populate the operations irrespective add/remove/modify event
            self.operations = get_operations(message.ops_dir.clone())?;
            Ok(Some(create_supported_operations(&message.ops_dir)?))

        // operation for child
        } else if message.ops_dir.eq(&self
            .cfg_dir
            .join("operations")
            .join("c8y")
            .join(get_child_id(&message.ops_dir)?))
        {
            self.children.insert(
                get_child_id(&message.ops_dir)?,
                get_operations(message.ops_dir.clone())?,
            );

            Ok(Some(create_supported_operations(&message.ops_dir)?))
        } else {
            Ok(None)
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
    http_proxy: &mut C8YHttpProxy,
    device_id: String,
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

            validate_and_publish_software_list(json_response, http_proxy, device_id).await?;
            Ok(vec![Message::new(&topic, smartrest_set_operation)])
        }
        OperationStatus::Failed => {
            let smartrest_set_operation =
                SmartRestSetOperationToFailed::from_thin_edge_json(response)?.to_smartrest()?;
            validate_and_publish_software_list(json_response, http_proxy, device_id).await?;
            Ok(vec![Message::new(&topic, smartrest_set_operation)])
        }
    }
}

async fn validate_and_publish_software_list(
    payload: &str,
    http_proxy: &mut C8YHttpProxy,
    device_id: String,
) -> Result<Vec<Message>, CumulocityMapperError> {
    let response = &SoftwareListResponse::from_json(payload)?;

    match response.status() {
        OperationStatus::Successful => {
            let c8y_software_list: C8yUpdateSoftwareListResponse = response.into();
            http_proxy
                .send_software_list_http(c8y_software_list, device_id)
                .await?;
        }

        OperationStatus::Failed => {
            error!("Received a failed software response: {payload}");
        }

        OperationStatus::Executing => {} // C8Y doesn't expect any message to be published
    }

    Ok(vec![])
}

/// Lists all the locally available child devices linked to this parent device.
///
/// The set of all locally available child devices is defined as any directory
/// created under "`config_dir`/operations/c8y" for example "/etc/tedge/operations/c8y"
pub fn get_local_child_devices_list(
    path: &Path,
) -> Result<std::collections::HashSet<String>, CumulocityMapperError> {
    Ok(fs::read_dir(path)
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

/// reads a json file to serde_json::Value
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

async fn create_tedge_agent_supported_ops(ops_dir: PathBuf) -> Result<(), ConversionError> {
    create_file_with_defaults(ops_dir.join("c8y_SoftwareUpdate"), None)?;
    create_file_with_defaults(ops_dir.join("c8y_Restart"), None)?;

    Ok(())
}

#[derive(Deserialize, Serialize, Debug)]
pub struct HealthStatus {
    #[serde(skip)]
    pub pid: u64,
    pub status: String,
}

pub fn check_tedge_agent_status(message: &Message) -> Result<bool, ConversionError> {
    if message.topic.name.eq(TEDGE_AGENT_HEALTH_TOPIC) {
        let status: HealthStatus = serde_json::from_str(message.payload_str()?)?;
        return Ok(status.status.eq("up"));
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use assert_json_diff::assert_json_include;
    use assert_matches::assert_matches;
    use c8y_http_proxy::handle::C8YHttpProxy;
    use c8y_http_proxy::messages::C8YRestRequest;
    use c8y_http_proxy::messages::C8YRestResult;
    use rand::prelude::Distribution;
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    use serde_json::json;
    use std::collections::HashMap;
    use tedge_actors::Builder;
    use tedge_actors::LoggingSender;
    use tedge_actors::MessageReceiver;
    use tedge_actors::Sender;
    use tedge_actors::SimpleMessageBox;
    use tedge_actors::SimpleMessageBoxBuilder;
    use tedge_mqtt_ext::Message;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;
    use tedge_utils::size_threshold::SizeThresholdExceededError;
    use test_case::test_case;

    use crate::config::C8yMapperConfig;
    use crate::converter::Converter;
    use crate::error::ConversionError;

    use super::CumulocityConverter;

    const OPERATIONS: &[&str] = &[
        "c8y_DownloadConfigFile",
        "c8y_LogfileRequest",
        "c8y_SoftwareUpdate",
        "c8y_Command",
    ];

    const EXPECTED_CHILD_DEVICES: &[&str] = &["child-0", "child-1", "child-2", "child-3"];

    #[tokio::test]
    async fn test_sync_alarms() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let alarm_topic = "tedge/alarms/critical/temperature_alarm";
        let alarm_payload = r#"{ "text": "Temperature very high" }"#;
        let alarm_message = Message::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

        // During the sync phase, alarms are not converted immediately, but only cached to be synced later
        assert!(converter.convert(&alarm_message).await.is_empty());

        let non_alarm_topic = "tedge/measurements";
        let non_alarm_payload = r#"{"temp": 1}"#;
        let non_alarm_message =
            Message::new(&Topic::new_unchecked(non_alarm_topic), non_alarm_payload);

        // But non-alarms are converted immediately, even during the sync phase
        assert!(!converter.convert(&non_alarm_message).await.is_empty());

        let internal_alarm_topic = "c8y-internal/alarms/major/pressure_alarm";
        let internal_alarm_payload = r#"{ "text": "Temperature very high" }"#;
        let internal_alarm_message = Message::new(
            &Topic::new_unchecked(internal_alarm_topic),
            internal_alarm_payload,
        );

        // During the sync phase, internal alarms are not converted, but only cached to be synced later
        assert!(converter.convert(&internal_alarm_message).await.is_empty());

        // When sync phase is complete, all pending alarms are returned
        let sync_messages = converter.sync_messages();
        assert_eq!(sync_messages.len(), 2);

        // The first message will be clear alarm message for pressure_alarm
        let alarm_message = sync_messages.get(0).unwrap();
        assert_eq!(
            alarm_message.topic.name,
            "tedge/alarms/major/pressure_alarm"
        );
        assert_eq!(alarm_message.payload_bytes().len(), 0); //Clear messages are empty messages

        // The second message will be the temperature_alarm
        let alarm_message = sync_messages.get(1).unwrap();
        assert_eq!(alarm_message.topic.name, alarm_topic);
        assert_eq!(alarm_message.payload_str().unwrap(), alarm_payload);

        // After the sync phase, the conversion of both non-alarms as well as alarms are done immediately
        assert!(!converter.convert(alarm_message).await.is_empty());
        assert!(!converter.convert(&non_alarm_message).await.is_empty());

        // But, even after the sync phase, internal alarms are not converted and just ignored, as they are purely internal
        assert!(converter.convert(&internal_alarm_message).await.is_empty());
    }

    #[tokio::test]
    async fn test_sync_child_alarms() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let alarm_topic = "tedge/alarms/critical/temperature_alarm/external_sensor";
        let alarm_payload = r#"{ "text": "Temperature very high" }"#;
        let alarm_message = Message::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

        // During the sync phase, alarms are not converted immediately, but only cached to be synced later
        assert!(converter.convert(&alarm_message).await.is_empty());

        let non_alarm_topic = "tedge/measurements/external_sensor";
        let non_alarm_payload = r#"{"temp": 1}"#;
        let non_alarm_message =
            Message::new(&Topic::new_unchecked(non_alarm_topic), non_alarm_payload);

        // But non-alarms are converted immediately, even during the sync phase
        assert!(!converter.convert(&non_alarm_message).await.is_empty());

        let internal_alarm_topic = "c8y-internal/alarms/major/pressure_alarm/external_sensor";
        let internal_alarm_payload = r#"{ "text": "Temperature very high" }"#;
        let internal_alarm_message = Message::new(
            &Topic::new_unchecked(internal_alarm_topic),
            internal_alarm_payload,
        );

        // During the sync phase, internal alarms are not converted, but only cached to be synced later
        assert!(converter.convert(&internal_alarm_message).await.is_empty());

        // When sync phase is complete, all pending alarms are returned
        let sync_messages = converter.sync_messages();
        assert_eq!(sync_messages.len(), 2);

        // The first message will be clear alarm message for pressure_alarm
        let alarm_message = sync_messages.get(0).unwrap();
        assert_eq!(
            alarm_message.topic.name,
            "tedge/alarms/major/pressure_alarm/external_sensor"
        );
        assert_eq!(alarm_message.payload_bytes().len(), 0); //Clear messages are empty messages

        // The second message will be the temperature_alarm
        let alarm_message = sync_messages.get(1).unwrap();
        assert_eq!(alarm_message.topic.name, alarm_topic);
        assert_eq!(alarm_message.payload_str().unwrap(), alarm_payload);

        // After the sync phase, the conversion of both non-alarms as well as alarms are done immediately
        assert!(!converter.convert(alarm_message).await.is_empty());
        assert!(!converter.convert(&non_alarm_message).await.is_empty());

        // But, even after the sync phase, internal alarms are not converted and just ignored, as they are purely internal
        assert!(converter.convert(&internal_alarm_message).await.is_empty());
    }

    #[tokio::test]
    async fn convert_measurement_with_child_id() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "tedge/measurements/child1";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,child1,child1,thin-edge.io-child",
        );
        let expected_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"externalSource":{"externalId":"child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
        );

        // Test the first output messages contains SmartREST and C8Y JSON.
        let out_first_messages = converter.convert(&in_message).await;
        assert_eq!(
            out_first_messages,
            vec![
                expected_smart_rest_message,
                expected_c8y_json_message.clone()
            ]
        );

        // Test the second output messages doesn't contain SmartREST child device creation.
        let out_second_messages = converter.convert(&in_message).await;
        assert_eq!(out_second_messages, vec![expected_c8y_json_message]);
    }

    #[tokio::test]
    async fn convert_first_measurement_invalid_then_valid_with_child_id() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "tedge/measurements/child1";
        let in_invalid_payload = r#"{"temp": invalid}"#;
        let in_valid_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_first_message = Message::new(&Topic::new_unchecked(in_topic), in_invalid_payload);
        let in_second_message = Message::new(&Topic::new_unchecked(in_topic), in_valid_payload);

        // First convert invalid Thin Edge JSON message.
        let out_first_messages = converter.convert(&in_first_message).await;
        let expected_error_message = Message::new(
            &Topic::new_unchecked("tedge/errors"),
            "Invalid JSON: expected value at line 1 column 10: `invalid}\n`",
        );
        assert_eq!(out_first_messages, vec![expected_error_message]);

        // Second convert valid Thin Edge JSON message.
        let out_second_messages = converter.convert(&in_second_message).await;
        let expected_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,child1,child1,thin-edge.io-child",
        );
        let expected_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"externalSource":{"externalId":"child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
        );
        assert_eq!(
            out_second_messages,
            vec![expected_smart_rest_message, expected_c8y_json_message]
        );
    }

    #[tokio::test]
    async fn convert_two_measurement_messages_given_different_child_id() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;

        // First message from "child1"
        let in_first_message = Message::new(
            &Topic::new_unchecked("tedge/measurements/child1"),
            in_payload,
        );
        let out_first_messages = converter.convert(&in_first_message).await;
        let expected_first_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,child1,child1,thin-edge.io-child",
        );
        let expected_first_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"externalSource":{"externalId":"child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
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
        let out_second_messages = converter.convert(&in_second_message).await;
        let expected_second_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,child2,child2,thin-edge.io-child",
        );
        let expected_second_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"externalSource":{"externalId":"child2","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
        );
        assert_eq!(
            out_second_messages,
            vec![
                expected_second_smart_rest_message,
                expected_second_c8y_json_message
            ]
        );
    }

    #[tokio::test]
    async fn check_c8y_threshold_packet_size() -> Result<(), anyhow::Error> {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let alarm_topic = "tedge/alarms/critical/temperature_alarm";
        let big_alarm_text = create_packet(1024 * 20);
        let alarm_payload = json!({ "text": big_alarm_text }).to_string();
        let alarm_message = Message::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

        assert_matches!(
            converter.try_convert(&alarm_message).await,
            Err(ConversionError::SizeThresholdExceeded(
                SizeThresholdExceededError {
                    size: _,
                    threshold: _
                }
            ))
        );
        Ok(())
    }

    #[tokio::test]
    async fn convert_event_with_known_fields_to_c8y_smartrest() -> Result<()> {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let event_topic = "tedge/events/click_event";
        let event_payload = r#"{ "text": "Someone clicked", "time": "2020-02-02T01:02:03+05:30" }"#;
        let event_message = Message::new(&Topic::new_unchecked(event_topic), event_payload);

        let converted_events = converter.convert(&event_message).await;
        assert_eq!(converted_events.len(), 1);
        let converted_event = converted_events.get(0).unwrap();
        assert_eq!(converted_event.topic.name, "c8y/s/us");

        assert_eq!(
            converted_event.payload_str()?,
            r#"400,click_event,"Someone clicked",2020-02-02T01:02:03+05:30"#
        );

        Ok(())
    }

    #[tokio::test]
    async fn convert_event_with_extra_fields_to_c8y_json() -> Result<()> {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let event_topic = "tedge/events/click_event";
        let event_payload = r#"{ "text": "tick", "foo": "bar" }"#;
        let event_message = Message::new(&Topic::new_unchecked(event_topic), event_payload);

        let converted_events = converter.convert(&event_message).await;
        assert_eq!(converted_events.len(), 1);
        let converted_event = converted_events.get(0).unwrap();
        assert_eq!(converted_event.topic.name, "c8y/event/events/create");
        let converted_c8y_json = json!({
            "type": "click_event",
            "text": "tick",
            "foo": "bar",
        });
        assert_eq!(converted_event.topic.name, "c8y/event/events/create");
        assert_json_include!(
            actual: serde_json::from_str::<serde_json::Value>(converted_event.payload_str()?)?,
            expected: converted_c8y_json
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_convert_big_event() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, mut http_proxy) = create_c8y_converter(&tmp_dir).await;
        tokio::spawn(async move {
            if let Some(C8YRestRequest::C8yCreateEvent(_)) = http_proxy.recv().await {
                let _ = http_proxy
                    .send(Ok(c8y_http_proxy::messages::C8YRestResponse::EventId(
                        "event-id".into(),
                    )))
                    .await;
            }
        });

        let event_topic = "tedge/events/click_event";
        let big_event_text = create_packet((16 + 1) * 1024); // Event payload > size_threshold
        let big_event_payload = json!({ "text": big_event_text }).to_string();
        let big_event_message = Message::new(&Topic::new_unchecked(event_topic), big_event_payload);

        println!("{:?}", converter.convert(&big_event_message).await);
        // assert!(converter.convert(&big_event_message).await.is_empty());
    }

    #[tokio::test]
    async fn test_convert_big_measurement() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let measurement_topic = "tedge/measurements";
        let big_measurement_payload = create_thin_edge_measurement(10 * 1024); // Measurement payload > size_threshold after converting to c8y json

        let big_measurement_message = Message::new(
            &Topic::new_unchecked(measurement_topic),
            big_measurement_payload,
        );
        let result = converter.convert(&big_measurement_message).await;

        let payload = result[0].payload_str().unwrap();
        assert!(payload.starts_with(
        r#"The payload {"temperature0":0,"temperature1":1,"temperature10" received on tedge/measurements after translation is"#
    ));
        assert!(payload.ends_with("greater than the threshold size of 16184."));
    }

    #[tokio::test]
    async fn test_convert_small_measurement() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let measurement_topic = "tedge/measurements";
        let big_measurement_payload = create_thin_edge_measurement(20); // Measurement payload size is 20 bytes

        let big_measurement_message = Message::new(
            &Topic::new_unchecked(measurement_topic),
            big_measurement_payload,
        );

        let result = converter.convert(&big_measurement_message).await;

        assert!(result[0]
            .payload_str()
            .unwrap()
            .contains(r#"{"temperature0":{"temperature0":{"value":0.0}}"#));
        assert!(result[0]
            .payload_str()
            .unwrap()
            .contains(r#""type":"ThinEdgeMeasurement""#));
    }

    #[tokio::test]
    async fn test_convert_big_measurement_for_child_device() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let measurement_topic = "tedge/measurements/child1";
        let big_measurement_payload = create_thin_edge_measurement(10 * 1024); // Measurement payload > size_threshold after converting to c8y json

        let big_measurement_message = Message::new(
            &Topic::new_unchecked(measurement_topic),
            big_measurement_payload,
        );

        let result = converter.convert(&big_measurement_message).await;

        let payload = result[0].payload_str().unwrap();
        assert!(payload.starts_with(
        r#"The payload {"temperature0":0,"temperature1":1,"temperature10" received on tedge/measurements/child1 after translation is"#
    ));
        assert!(payload.ends_with("greater than the threshold size of 16184."));
    }

    #[tokio::test]
    async fn test_convert_small_measurement_for_child_device() {
        let tmp_dir = TempTedgeDir::new();
        let measurement_topic = "tedge/measurements/child1";
        let big_measurement_payload = create_thin_edge_measurement(20); // Measurement payload size is 20 bytes

        let big_measurement_message = Message::new(
            &Topic::new_unchecked(measurement_topic),
            big_measurement_payload,
        );
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let result = converter.convert(&big_measurement_message).await;

        let payload1 = &result[0].payload_str().unwrap();
        let payload2 = &result[1].payload_str().unwrap();

        assert!(payload1.contains("101,child1,child1,thin-edge.io-child"));
        assert!(payload2 .contains(
        r#"{"externalSource":{"externalId":"child1","type":"c8y_Serial"},"temperature0":{"temperature0":{"value":0.0}},"#
    ));
        assert!(payload2.contains(r#""type":"ThinEdgeMeasurement""#));
    }

    #[tokio::test]
    async fn translate_service_monitor_message_for_child_device() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "tedge/health/child1/child-service-c8y";
        let in_payload = r#"{"pid":"1234","status":"up","time":"2021-11-16T17:45:40.571760714+01:00","type":"thin-edge.io"}"#;
        let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_child_create_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,child1,child1,thin-edge.io-child",
        );

        let expected_service_monitor_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us/child1"),
            r#"102,test-device_child1_child-service-c8y,"thin-edge.io",child-service-c8y,"up""#,
        );

        // Test the first output messages contains SmartREST and C8Y JSON.
        let out_first_messages = converter.convert(&in_message).await;

        assert_eq!(
            out_first_messages,
            vec![
                expected_child_create_smart_rest_message,
                expected_service_monitor_smart_rest_message.clone()
            ]
        );
    }

    #[tokio::test]
    async fn translate_service_monitor_message_for_thin_edge_device() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "tedge/health/test-tedge-mapper-c8y";
        let in_payload = r#"{"pid":"1234","status":"up","time":"2021-11-16T17:45:40.571760714+01:00","type":"thin-edge.io"}"#;
        let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_service_monitor_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            r#"102,test-device_test-tedge-mapper-c8y,"thin-edge.io",test-tedge-mapper-c8y,"up""#,
        );

        // Test the output messages contains SmartREST and C8Y JSON.
        let out_messages = converter.convert(&in_message).await;

        assert_eq!(
            out_messages,
            vec![expected_service_monitor_smart_rest_message]
        );
    }

    #[tokio::test]
    async fn test_execute_operation_is_not_blocked() {
        let tmp_dir = TempTedgeDir::new();
        let (converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let now = std::time::Instant::now();
        converter
            .execute_operation(
                "5",
                "sleep",
                "sleep_ten",
                tokio::time::Duration::from_secs(10),
                tokio::time::Duration::from_secs(1),
            )
            .await
            .unwrap();
        converter
            .execute_operation(
                "5",
                "sleep",
                "sleep_twenty",
                tokio::time::Duration::from_secs(20),
                tokio::time::Duration::from_secs(1),
            )
            .await
            .unwrap();

        // a result between now and elapsed that is not 0 probably means that the operations are
        // blocking and that you probably removed a tokio::spawn handle (;
        assert_eq!(now.elapsed().as_secs(), 0);
    }

    #[tokio::test]
    async fn ignore_operations_for_child_device() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let output = converter
            .process_smartrest("528,childId,software_a,version_a,url_a,install")
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

        let (mut converter, _http_proxy) = create_c8y_converter(&ttd).await;

        let output_messages = converter
            .process_smartrest(cloud_child_devices)
            .await
            .unwrap();

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

    async fn create_c8y_converter(
        tmp_dir: &TempTedgeDir,
    ) -> (
        CumulocityConverter,
        SimpleMessageBox<C8YRestRequest, C8YRestResult>,
    ) {
        tmp_dir.dir("operations").dir("c8y");
        tmp_dir.dir("tedge").dir("agent");

        let device_id = "test-device".into();
        let device_type = "test-device-type".into();
        let service_type = "service".into();
        let c8y_host = "test.c8y.io".into();
        let mut topics = C8yMapperConfig::internal_topic_filter(&tmp_dir.to_path_buf()).unwrap();
        topics.add_all(C8yMapperConfig::default_external_topic_filter());

        let config = C8yMapperConfig::new(
            tmp_dir.to_path_buf(),
            tmp_dir.utf8_path_buf(),
            device_id,
            device_type,
            service_type,
            c8y_host,
            topics,
        );

        let mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
            SimpleMessageBoxBuilder::new("MQTT", 5);
        let mqtt_publisher = LoggingSender::new("MQTT".into(), mqtt_builder.build().sender_clone());

        let mut c8y_proxy_builder: SimpleMessageBoxBuilder<C8YRestRequest, C8YRestResult> =
            SimpleMessageBoxBuilder::new("C8Y", 1);
        let http_proxy = C8YHttpProxy::new("C8Y", &mut c8y_proxy_builder);

        let converter = CumulocityConverter::new(config, mqtt_publisher, http_proxy).unwrap();

        (converter, c8y_proxy_builder.build())
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

    fn create_thin_edge_measurement(size: usize) -> String {
        let mut map = serde_json::Map::new();
        let data = r#""temperature":25"#;
        let loops = size / data.len();
        for i in 0..loops {
            map.insert(format!("temperature{i}"), json!(i));
        }
        let obj = serde_json::Value::Object(map);
        serde_json::to_string(&obj).unwrap()
    }
}
