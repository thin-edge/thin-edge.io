use super::alarm_converter::AlarmConverter;
use super::config::C8yMapperConfig;
use super::config::MQTT_MESSAGE_SIZE_THRESHOLD;
use super::error::CumulocityMapperError;
use super::service_monitor;
use crate::actor::CmdId;
use crate::actor::IdDownloadRequest;
use crate::dynamic_discovery::DiscoverOp;
use crate::error::ConversionError;
use crate::json;
use c8y_api::http_proxy::C8yEndPoint;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::json_c8y::C8yUpdateSoftwareListResponse;
use c8y_api::smartrest::error::OperationsError;
use c8y_api::smartrest::error::SmartRestDeserializerError;
use c8y_api::smartrest::inventory::child_device_creation_message;
use c8y_api::smartrest::inventory::service_creation_message;
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
use c8y_api::smartrest::smartrest_deserializer::SmartRestOperationVariant;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRestartRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestUpdateSoftware;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRestGetPendingOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRestSerializer;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToExecuting;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToFailed;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToSuccessful;
use c8y_api::smartrest::topic::publish_topic_from_ancestors;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_api::smartrest::topic::MapperSubscribeTopic;
use c8y_api::smartrest::topic::SMARTREST_PUBLISH_TOPIC;
use c8y_auth_proxy::url::ProxyUrlGenerator;
use c8y_http_proxy::handle::C8YHttpProxy;
use logged_command::LoggedCommand;
use plugin_sm::operation_logs::OperationLogs;
use plugin_sm::operation_logs::OperationLogsError;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Map;
use serde_json::Value;
use service_monitor::convert_health_status_message;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tedge_actors::LoggingSender;
use tedge_actors::Sender;
use tedge_api::entity_store;
use tedge_api::entity_store::EntityExternalId;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::entity_store::EntityType;
use tedge_api::entity_store::Error;
use tedge_api::entity_store::InvalidExternalIdError;
use tedge_api::event::error::ThinEdgeJsonDeserializerError;
use tedge_api::event::ThinEdgeEvent;
use tedge_api::messages::CommandStatus;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::topic::RequestTopic;
use tedge_api::topic::ResponseTopic;
use tedge_api::DownloadInfo;
use tedge_api::EntityStore;
use tedge_api::Jsonify;
use tedge_api::OperationStatus;
use tedge_api::RestartCommand;
use tedge_api::SoftwareListRequest;
use tedge_api::SoftwareListResponse;
use tedge_api::SoftwareUpdateResponse;
use tedge_config::TEdgeConfigError;
use tedge_mqtt_ext::Message;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::create_file_with_defaults;
use tedge_utils::size_threshold::SizeThreshold;
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use tokio::time::Duration;
use tracing::debug;
use tracing::log::error;
use tracing::trace;

const C8Y_CLOUD: &str = "c8y";
const SUPPORTED_OPERATIONS_DIRECTORY: &str = "operations";
const INTERNAL_ALARMS_TOPIC: &str = "c8y-internal/alarms/";
const C8Y_JSON_MQTT_EVENTS_TOPIC: &str = "c8y/event/events/create";
const TEDGE_AGENT_LOG_DIR: &str = "tedge/agent";
const CREATE_EVENT_SMARTREST_CODE: u16 = 400;
const DEFAULT_EVENT_TYPE: &str = "ThinEdgeEvent";
const FORBIDDEN_ID_CHARS: [char; 3] = ['/', '+', '#'];

#[derive(Debug)]
pub struct MapperConfig {
    pub out_topic: Topic,
    pub errors_topic: Topic,
}

impl CumulocityConverter {
    pub async fn convert(&mut self, input: &Message) -> Vec<Message> {
        let messages_or_err = self.try_convert(input).await;
        self.wrap_errors(messages_or_err)
    }

    pub fn wrap_errors(
        &self,
        messages_or_err: Result<Vec<Message>, ConversionError>,
    ) -> Vec<Message> {
        messages_or_err.unwrap_or_else(|error| vec![self.new_error_message(error)])
    }

    pub fn wrap_error(&self, message_or_err: Result<Message, ConversionError>) -> Message {
        message_or_err.unwrap_or_else(|error| self.new_error_message(error))
    }

    pub fn new_error_message(&self, error: ConversionError) -> Message {
        error!("Mapping error: {}", error);
        Message::new(&self.get_mapper_config().errors_topic, error.to_string())
    }

    /// This function will be the first method that's called on the converter after it's instantiated.
    /// Return any initialization messages that must be processed before the converter starts converting regular messages.
    pub fn init_messages(&mut self) -> Vec<Message> {
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

    pub fn process_operation_update_message(&mut self, message: DiscoverOp) -> Message {
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
    pub config: C8yMapperConfig,
    pub(crate) mapper_config: MapperConfig,
    pub device_name: String,
    pub(crate) device_topic_id: EntityTopicId,
    pub(crate) device_type: String,
    alarm_converter: AlarmConverter,
    pub operations: Operations,
    operation_logs: OperationLogs,
    mqtt_publisher: LoggingSender<MqttMessage>,
    pub http_proxy: C8YHttpProxy,
    pub cfg_dir: PathBuf,
    pub ops_dir: PathBuf,
    pub children: HashMap<String, Operations>,
    pub service_type: String,
    pub c8y_endpoint: C8yEndPoint,
    pub mqtt_schema: MqttSchema,
    pub entity_store: EntityStore,
    pub auth_proxy: ProxyUrlGenerator,
    pub downloader_sender: LoggingSender<IdDownloadRequest>,
    pub pending_operations: HashMap<CmdId, SmartRestOperationVariant>,
    pub inventory_model: Value, // Holds a live view of aggregated inventory, derived from various twin data
}

impl CumulocityConverter {
    pub fn new(
        config: C8yMapperConfig,
        mqtt_publisher: LoggingSender<MqttMessage>,
        http_proxy: C8YHttpProxy,
        auth_proxy: ProxyUrlGenerator,
        downloader_sender: LoggingSender<IdDownloadRequest>,
    ) -> Result<Self, CumulocityConverterBuildError> {
        let device_id = config.device_id.clone();
        let device_topic_id = config.device_topic_id.clone();
        let device_type = config.device_type.clone();
        let service_type = config.service_type.clone();
        let c8y_host = config.c8y_host.clone();
        let cfg_dir = config.config_dir.clone();

        let size_threshold = SizeThreshold(MQTT_MESSAGE_SIZE_THRESHOLD);

        let ops_dir = config.ops_dir.clone();
        let operations = Operations::try_new(ops_dir.clone())?;
        let children = get_child_ops(ops_dir.clone())?;

        let alarm_converter = AlarmConverter::new();

        let log_dir = config.logs_path.join(TEDGE_AGENT_LOG_DIR);
        let operation_logs = OperationLogs::try_new(log_dir.into())?;

        let c8y_endpoint = C8yEndPoint::new(&c8y_host, &device_id);

        let mapper_config = MapperConfig {
            out_topic: Topic::new_unchecked("c8y/measurement/measurements/create"),
            errors_topic: Topic::new_unchecked("tedge/errors"),
        };

        let main_device = entity_store::EntityRegistrationMessage::main_device(device_id.clone());
        let entity_store = EntityStore::with_main_device_and_default_service_type(
            main_device,
            service_type.clone(),
            Self::map_to_c8y_external_id,
            Self::validate_external_id,
        )
        .unwrap();

        let inventory_model = json!({
            "name": device_id.clone(),
            "type": device_type.clone(),
        });

        Ok(CumulocityConverter {
            size_threshold,
            config,
            mapper_config,
            device_name: device_id,
            device_topic_id,
            device_type,
            alarm_converter,
            operations,
            operation_logs,
            http_proxy,
            cfg_dir,
            ops_dir,
            children,
            mqtt_publisher,
            service_type,
            c8y_endpoint,
            mqtt_schema: MqttSchema::default(),
            entity_store,
            auth_proxy,
            downloader_sender,
            pending_operations: HashMap::new(),
            inventory_model,
        })
    }

    pub fn try_convert_entity_registration(
        &mut self,
        input: &EntityRegistrationMessage,
    ) -> Result<Vec<Message>, ConversionError> {
        // Parse the optional fields
        let display_name = input.other.get("name").and_then(|v| v.as_str());
        let display_type = input.other.get("type").and_then(|v| v.as_str());

        let entity_topic_id = &input.topic_id;
        let external_id = self
            .entity_store
            .get(entity_topic_id)
            .map(|e| &e.external_id)
            .ok_or_else(|| Error::UnknownEntity(entity_topic_id.to_string()))?;
        match input.r#type {
            EntityType::MainDevice => {
                self.entity_store.update(input.clone())?;
                Ok(vec![])
            }
            EntityType::ChildDevice => {
                let ancestors_external_ids =
                    self.entity_store.ancestors_external_ids(entity_topic_id)?;
                let child_creation_message = child_device_creation_message(
                    external_id.as_ref(),
                    display_name,
                    display_type,
                    &ancestors_external_ids,
                );
                Ok(vec![child_creation_message])
            }
            EntityType::Service => {
                let ancestors_external_ids =
                    self.entity_store.ancestors_external_ids(entity_topic_id)?;

                let service_creation_message = service_creation_message(
                    external_id.as_ref(),
                    display_name.unwrap_or_else(|| {
                        entity_topic_id
                            .default_service_name()
                            .unwrap_or(external_id.as_ref())
                    }),
                    display_type.unwrap_or(&self.service_type),
                    "up",
                    &ancestors_external_ids,
                );
                Ok(vec![service_creation_message])
            }
        }
    }

    /// Return the SmartREST publish topic for the given entity
    /// derived from its ancestors.
    pub fn smartrest_publish_topic_for_entity(
        &self,
        entity_topic_id: &EntityTopicId,
    ) -> Result<Topic, ConversionError> {
        let entity = self.entity_store.try_get(entity_topic_id)?;

        let mut ancestors_external_ids =
            self.entity_store.ancestors_external_ids(entity_topic_id)?;
        ancestors_external_ids.insert(0, entity.external_id.as_ref().into());
        Ok(publish_topic_from_ancestors(&ancestors_external_ids))
    }

    /// Generates external ID of the given entity.
    ///
    /// The external id is generated by transforming the EntityTopicId
    /// by replacing the `/` characters with `:` and then adding the
    /// main device id as a prefix, to namespace all the entities under that device.
    ///
    /// # Examples
    /// - `device/main//` => `DEVICE_COMMON_NAME`
    /// - `device/child001//` => `DEVICE_COMMON_NAME:device:child001`
    /// - `device/child001/service/service001` => `DEVICE_COMMON_NAME:device:child001:service:service001`
    /// - `factory01/hallA/packaging/belt001` => `DEVICE_COMMON_NAME:factory01:hallA:packaging:belt001`
    pub fn map_to_c8y_external_id(
        entity_topic_id: &EntityTopicId,
        main_device_xid: &EntityExternalId,
    ) -> EntityExternalId {
        if entity_topic_id.is_default_main_device() {
            main_device_xid.clone()
        } else {
            format!(
                "{}:{}",
                main_device_xid.as_ref(),
                entity_topic_id
                    .to_string()
                    .trim_end_matches('/')
                    .replace('/', ":")
            )
            .into()
        }
    }

    /// Returns the `device_name` from the `EntityExternalId`
    /// if it follows the default naming scheme `MAIN_DEVICE_COMMON_NAME:device:device_name`,
    /// else returns its `String` representation
    pub fn default_device_name_from_external_id(&self, external_id: &EntityExternalId) -> String {
        let _main_device_id = &self.device_name;
        match external_id.as_ref().split(':').collect::<Vec<&str>>()[..] {
            [_main_device_id, "device", device_id, "service", _] => device_id.into(),
            [_main_device_id, "device", device_id] => device_id.into(),
            _ => external_id.into(),
        }
    }

    /// Validates if the provided id contains any invalid characters and
    /// returns a valid EntityExternalId if the validation passes,
    /// else returns InvalidExternalIdError
    pub fn validate_external_id(id: &str) -> Result<EntityExternalId, InvalidExternalIdError> {
        let forbidden_chars = HashSet::from(FORBIDDEN_ID_CHARS);
        for c in id.chars() {
            if forbidden_chars.contains(&c) {
                return Err(InvalidExternalIdError {
                    external_id: id.into(),
                    invalid_char: c,
                });
            }
        }
        Ok(id.into())
    }

    fn try_convert_measurement(
        &mut self,
        source: &EntityTopicId,
        input: &Message,
        measurement_type: &str,
    ) -> Result<Vec<Message>, ConversionError> {
        let mut mqtt_messages: Vec<Message> = Vec::new();

        if let Some(entity) = self.entity_store.get(source) {
            // Need to check if the input Thin Edge JSON is valid before adding a child ID to list
            let c8y_json_payload =
                json::from_thin_edge_json(input.payload_str()?, entity, measurement_type)?;

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
        };

        Ok(mqtt_messages)
    }

    async fn try_convert_event(
        &mut self,
        source: &EntityTopicId,
        input: &Message,
        event_type: &str,
    ) -> Result<Vec<Message>, ConversionError> {
        let mut messages = Vec::new();

        let event_type = match event_type.is_empty() {
            true => DEFAULT_EVENT_TYPE,
            false => event_type,
        };

        if let Some(entity) = self.entity_store.get(source) {
            let mqtt_topic = input.topic.name.clone();
            let mqtt_payload = input.payload_str().map_err(|e| {
                ThinEdgeJsonDeserializerError::FailedToParsePayloadToString {
                    topic: mqtt_topic.clone(),
                    error: e.to_string(),
                }
            })?;

            let tedge_event =
                ThinEdgeEvent::try_from(event_type, entity, mqtt_payload).map_err(|e| {
                    ThinEdgeJsonDeserializerError::FailedToParseJsonPayload {
                        topic: mqtt_topic.clone(),
                        error: e.to_string(),
                        payload: mqtt_payload.chars().take(50).collect(),
                    }
                })?;

            let c8y_event = C8yCreateEvent::from(tedge_event);

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
            } else {
                // The message must be sent over HTTP
                let _ = self.http_proxy.send_event(c8y_event).await?;
                return Ok(vec![]);
            }
        };

        Ok(messages)
    }

    pub fn process_alarm_messages(
        &mut self,
        source: &EntityTopicId,
        input: &Message,
        alarm_type: &str,
    ) -> Result<Vec<Message>, ConversionError> {
        self.size_threshold.validate(input)?;

        let mqtt_messages = self.alarm_converter.try_convert_alarm(
            source,
            input,
            alarm_type,
            &self.entity_store,
        )?;

        Ok(mqtt_messages)
    }

    pub async fn process_health_status_message(
        &mut self,
        entity: &EntityTopicId,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        let mut mqtt_messages: Vec<Message> = Vec::new();

        // Send the init messages
        if self.is_message_tedge_agent_up(message)? {
            create_tedge_agent_supported_ops(&self.ops_dir).await?;
            mqtt_messages.push(create_get_software_list_message()?);
        }

        let entity_metadata = self
            .entity_store
            .get(entity)
            .expect("entity was registered");

        let ancestors_external_ids = self.entity_store.ancestors_external_ids(entity)?;
        let mut message =
            convert_health_status_message(entity_metadata, &ancestors_external_ids, message);

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
            Some(device_id) => {
                match get_smartrest_template_id(payload).as_str() {
                    "522" => self.convert_log_upload_request(payload),
                    "524" => self.convert_config_update_request(payload).await,
                    "526" => self.convert_config_snapshot_request(payload),
                    "528" if device_id == self.device_name => {
                        self.forward_software_request(payload).await
                    }
                    "510" => self.forward_restart_request(payload),
                    template if device_id == self.device_name => {
                        self.forward_operation_request(payload, template).await
                    }
                    "106" if device_id != self.device_name => {
                        self.register_child_device_supported_operations(payload)
                    }
                    _ => {
                        // Ignore any other child device incoming request as not yet supported
                        debug!("Ignored. Message not yet supported: {payload}");
                        Ok(vec![])
                    }
                }
            }
            None => {
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

        software_update_request
            .update_list
            .iter_mut()
            .for_each(|modules| {
                modules.modules.iter_mut().for_each(|module| {
                    if let Some(url) = &mut module.url {
                        *url = if let Some(cumulocity_url) =
                            self.c8y_endpoint.maybe_tenant_url(url.url())
                        {
                            DownloadInfo::new(self.auth_proxy.proxy_url(cumulocity_url).as_ref())
                        } else {
                            DownloadInfo::new(url.url())
                        };
                    }
                });
            });

        Ok(vec![Message::new(
            &topic,
            software_update_request.to_json(),
        )])
    }

    fn forward_restart_request(
        &mut self,
        smartrest: &str,
    ) -> Result<Vec<Message>, CumulocityMapperError> {
        let request = SmartRestRestartRequest::from_smartrest(smartrest)?;
        let device_id = &request.device.into();
        let target = self.entity_store.try_get_by_external_id(device_id)?;
        let command = RestartCommand::new(target.topic_id.clone());
        let message = command.command_message(&self.mqtt_schema);
        Ok(vec![message])
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
            let child_external_id = match CumulocityConverter::validate_external_id(child_id) {
                Ok(name) => name,
                Err(err) => {
                    error!(
                        "Child device directory: {} ignored due to {}",
                        &child_id, err
                    );
                    continue;
                }
            };
            let child_topic_id =
                EntityTopicId::default_child_device(child_external_id.as_ref()).unwrap();
            let child_device_reg_msg = EntityRegistrationMessage {
                topic_id: child_topic_id,
                external_id: Some(child_external_id.clone()),
                r#type: EntityType::ChildDevice,
                parent: None,
                other: json!({ "name": child_external_id.as_ref() }),
            };
            let mut reg_messages = self
                .register_and_convert_entity(&child_device_reg_msg)
                .unwrap();

            messages_vec.append(&mut reg_messages);
        }
        // loop over all local child devices and update the operations
        for child_id in local_child_devices {
            let child_external_id = match CumulocityConverter::validate_external_id(&child_id) {
                Ok(name) => name,
                Err(err) => {
                    error!(
                        "Supported operations of child device directory: {} ignored due to {}",
                        &child_id, err
                    );
                    continue;
                }
            };
            // update the children cache with the operations supported
            let ops = Operations::try_new(path_to_child_devices.join(&child_id))?;
            self.children
                .insert(child_external_id.clone().into(), ops.clone());
            let ops_msg = ops.create_smartrest_ops_message()?;
            let topic = C8yTopic::ChildSmartRestResponse(child_external_id.into()).to_topic()?;
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

impl CumulocityConverter {
    fn get_mapper_config(&self) -> &MapperConfig {
        &self.mapper_config
    }

    pub async fn try_convert(
        &mut self,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        debug!("Mapping message on topic: {}", message.topic.name);
        trace!("Message content: {:?}", message.payload_str());
        match self.mqtt_schema.entity_channel_of(&message.topic) {
            Ok((source, channel)) => self.try_convert_te_topics(source, channel, message).await,
            Err(_) => self.try_convert_tedge_topics(message).await,
        }
    }

    async fn try_convert_te_topics(
        &mut self,
        source: EntityTopicId,
        channel: Channel,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        let mut registration_messages: Vec<Message> = vec![];
        match &channel {
            Channel::EntityMetadata => {
                if let Ok(register_message) = EntityRegistrationMessage::try_from(message) {
                    match self.entity_store.update(register_message.clone()) {
                        Err(e) => {
                            error!("Entity registration failed: {e}");
                        }
                        Ok(affected_entities) if !affected_entities.is_empty() => {
                            let mut c8y_message =
                                self.try_convert_entity_registration(&register_message)?;
                            registration_messages.append(&mut c8y_message);
                        }
                        Ok(_) => {}
                    }
                }
            }
            _ => {
                // if device is unregistered register using auto-registration
                if self.entity_store.get(&source).is_none() {
                    let auto_registration_messages =
                        self.entity_store.auto_register_entity(&source)?;

                    for auto_registration_message in &auto_registration_messages {
                        registration_messages.append(
                            &mut self.register_and_convert_entity(auto_registration_message)?,
                        );
                    }
                }
            }
        }

        let mut messages = match &channel {
            Channel::EntityTwinData { fragment_key } => {
                self.try_convert_entity_twin_data(&source, message, fragment_key)?
            }

            Channel::Measurement { measurement_type } => {
                self.try_convert_measurement(&source, message, measurement_type)?
            }

            Channel::Event { event_type } => {
                self.try_convert_event(&source, message, event_type).await?
            }

            Channel::Alarm { alarm_type } => {
                self.process_alarm_messages(&source, message, alarm_type)?
            }

            Channel::Command { .. } if message.payload_bytes().is_empty() => {
                // The command has been fully processed
                vec![]
            }

            Channel::CommandMetadata {
                operation: OperationType::Restart,
            } => self.register_restart_operation(&source).await?,
            Channel::Command {
                operation: OperationType::Restart,
                cmd_id,
            } => {
                self.publish_restart_operation_status(&source, cmd_id, message)
                    .await?
            }

            Channel::CommandMetadata {
                operation: OperationType::LogUpload,
            } => self.convert_log_metadata(&source, message)?,

            Channel::Command {
                operation: OperationType::LogUpload,
                cmd_id,
            } => {
                self.handle_log_upload_state_change(&source, cmd_id, message)
                    .await?
            }

            Channel::CommandMetadata {
                operation: OperationType::ConfigSnapshot,
            } => self.convert_config_snapshot_metadata(&source, message)?,
            Channel::Command {
                operation: OperationType::ConfigSnapshot,
                cmd_id,
            } => {
                self.handle_config_snapshot_state_change(&source, cmd_id, message)
                    .await?
            }

            Channel::CommandMetadata {
                operation: OperationType::ConfigUpdate,
            } => self.convert_config_update_metadata(&source, message)?,
            Channel::Command {
                operation: OperationType::ConfigUpdate,
                cmd_id,
            } => {
                self.handle_config_update_state_change(&source, cmd_id, message)
                    .await?
            }
            Channel::Health => self.process_health_status_message(&source, message).await?,

            _ => vec![],
        };

        registration_messages.append(&mut messages);
        Ok(registration_messages)
    }

    pub fn register_and_convert_entity(
        &mut self,
        registration_message: &EntityRegistrationMessage,
    ) -> Result<Vec<Message>, ConversionError> {
        let entity_topic_id = &registration_message.topic_id;
        self.entity_store.update(registration_message.clone())?;
        if registration_message.r#type == EntityType::ChildDevice {
            self.children.insert(
                self.entity_store
                    .get(entity_topic_id)
                    .expect("Should have been registered in the previous step")
                    .external_id
                    .as_ref()
                    .into(),
                Operations::default(),
            );
        }

        let mut registration_messages = vec![];
        registration_messages.push(self.convert_entity_registration_message(registration_message));
        let mut c8y_message = self.try_convert_entity_registration(registration_message)?;
        registration_messages.append(&mut c8y_message);

        Ok(registration_messages)
    }

    fn convert_entity_registration_message(&self, value: &EntityRegistrationMessage) -> Message {
        let entity_topic_id = value.topic_id.clone();

        let mut register_payload: Map<String, Value> = Map::new();

        let entity_type = match value.r#type {
            EntityType::MainDevice => "device",
            EntityType::ChildDevice => "child-device",
            EntityType::Service => "service",
        };
        register_payload.insert("@type".into(), Value::String(entity_type.to_string()));

        if let Some(external_id) = &value.external_id {
            register_payload.insert("@id".into(), Value::String(external_id.as_ref().into()));
        }

        if let Some(parent_id) = &value.parent {
            register_payload.insert("@parent".into(), Value::String(parent_id.to_string()));
        }

        if let Value::Object(other_keys) = value.other.clone() {
            register_payload.extend(other_keys)
        }

        Message::new(
            &Topic::new(&format!("{}/{entity_topic_id}", self.mqtt_schema.root)).unwrap(),
            serde_json::to_string(&Value::Object(register_payload)).unwrap(),
        )
        .with_retain()
    }

    async fn try_convert_tedge_topics(
        &mut self,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        let messages = match &message.topic {
            topic if topic.name.starts_with(INTERNAL_ALARMS_TOPIC) => {
                self.alarm_converter.process_internal_alarm(message);
                Ok(vec![])
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
                Ok(MapperSubscribeTopic::C8yTopic(_)) => self.parse_c8y_topics(message).await,
                _ => {
                    error!("Unsupported topic: {}", message.topic.name);
                    Ok(vec![])
                }
            },
        }?;

        Ok(messages)
    }

    fn try_init_messages(&mut self) -> Result<Vec<Message>, ConversionError> {
        let mut messages = self.parse_base_inventory_file()?;

        let supported_operations_message =
            self.create_supported_operations(&self.cfg_dir.join("operations").join("c8y"))?;

        let device_data_message = self.inventory_device_type_update_message()?;

        let pending_operations_message = create_get_pending_operations_message()?;

        let cloud_child_devices_message = create_request_for_cloud_child_devices();

        messages.append(&mut vec![
            supported_operations_message,
            device_data_message,
            pending_operations_message,
            cloud_child_devices_message,
        ]);
        Ok(messages)
    }

    fn create_supported_operations(&self, path: &Path) -> Result<Message, ConversionError> {
        if is_child_operation_path(path) {
            // operations for child
            let child_id = get_child_id(&path.to_path_buf())?;
            let child_external_id = Self::validate_external_id(&child_id)?;

            let topic = C8yTopic::ChildSmartRestResponse(child_external_id.into()).to_topic()?;
            Ok(Message::new(
                &topic,
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

    pub fn sync_messages(&mut self) -> Vec<Message> {
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
            Ok(Some(self.create_supported_operations(&message.ops_dir)?))

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

            Ok(Some(self.create_supported_operations(&message.ops_dir)?))
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

fn create_get_software_list_message() -> Result<Message, ConversionError> {
    let request = SoftwareListRequest::default();
    let topic = Topic::new(RequestTopic::SoftwareListRequest.as_str())?;
    let payload = request.to_json();
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

fn create_request_for_cloud_child_devices() -> Message {
    Message::new(&Topic::new_unchecked("c8y/s/us"), "105")
}

impl CumulocityConverter {
    async fn register_restart_operation(
        &self,
        target: &EntityTopicId,
    ) -> Result<Vec<Message>, ConversionError> {
        match self.entity_store.get(target) {
            None => {
                error!("Fail to register `restart` operation for unknown device: {target}");
                Ok(vec![])
            }
            Some(device) => {
                let ops_dir = match device.r#type {
                    EntityType::MainDevice => self.ops_dir.clone(),
                    EntityType::ChildDevice => {
                        let child_dir_name = device.external_id.as_ref();
                        self.ops_dir.clone().join(child_dir_name)
                    }
                    EntityType::Service => {
                        error!("Unsupported `restart` operation for a service: {target}");
                        return Ok(vec![]);
                    }
                };
                let ops_file = ops_dir.join("c8y_Restart");
                create_directory_with_defaults(&ops_dir)?;
                create_file_with_defaults(ops_file, None)?;
                let device_operations = self.create_supported_operations(&ops_dir)?;
                Ok(vec![device_operations])
            }
        }
    }

    async fn publish_restart_operation_status(
        &mut self,
        target: &EntityTopicId,
        cmd_id: &str,
        message: &Message,
    ) -> Result<Vec<Message>, CumulocityMapperError> {
        let command = match RestartCommand::try_from(
            target.clone(),
            cmd_id.to_owned(),
            message.payload_bytes(),
        )? {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok(vec![]);
            }
        };
        let topic = self
            .entity_store
            .get(target)
            .and_then(C8yTopic::smartrest_response_topic)
            .ok_or_else(|| Error::UnknownEntity(target.to_string()))?;

        match command.status() {
            CommandStatus::Executing => {
                let smartrest_set_operation = SmartRestSetOperationToExecuting::new(
                    CumulocitySupportedOperations::C8yRestartRequest,
                )
                .to_smartrest()?;

                Ok(vec![Message::new(&topic, smartrest_set_operation)])
            }
            CommandStatus::Successful => {
                let smartrest_set_operation = SmartRestSetOperationToSuccessful::new(
                    CumulocitySupportedOperations::C8yRestartRequest,
                )
                .to_smartrest()?;

                Ok(vec![
                    command.clearing_message(&self.mqtt_schema),
                    Message::new(&topic, smartrest_set_operation),
                ])
            }
            CommandStatus::Failed { ref reason } => {
                let smartrest_set_operation = SmartRestSetOperationToFailed::new(
                    CumulocitySupportedOperations::C8yRestartRequest,
                    format!("Restart Failed: {}", reason),
                )
                .to_smartrest()?;
                Ok(vec![
                    command.clearing_message(&self.mqtt_schema),
                    Message::new(&topic, smartrest_set_operation),
                ])
            }
            _ => {
                // The other states are ignored
                Ok(vec![])
            }
        }
    }

    pub fn is_message_tedge_agent_up(&self, message: &Message) -> Result<bool, ConversionError> {
        let main_device_topic_id = self.entity_store.main_device();
        let tedge_agent_topic_id = main_device_topic_id
            .to_default_service_topic_id("tedge-agent")
            .expect("main device topic needs to fit default MQTT scheme");
        let tedge_agent_health_topic = self
            .mqtt_schema
            .topic_for(tedge_agent_topic_id.entity(), &Channel::Health);

        if message.topic == tedge_agent_health_topic {
            let status: HealthStatus = serde_json::from_str(message.payload_str()?)?;
            return Ok(status.status.eq("up"));
        }
        Ok(false)
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

async fn create_tedge_agent_supported_ops(ops_dir: &Path) -> Result<(), ConversionError> {
    create_file_with_defaults(ops_dir.join("c8y_SoftwareUpdate"), None)?;

    Ok(())
}

#[derive(Deserialize, Serialize, Debug)]
pub struct HealthStatus {
    #[serde(skip)]
    pub pid: u64,
    pub status: String,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::CumulocityConverter;
    use crate::actor::IdDownloadRequest;
    use crate::actor::IdDownloadResult;
    use crate::config::C8yMapperConfig;
    use crate::error::ConversionError;
    use crate::Capabilities;
    use anyhow::Result;
    use assert_json_diff::assert_json_eq;
    use assert_json_diff::assert_json_include;
    use assert_matches::assert_matches;
    use c8y_auth_proxy::url::ProxyUrlGenerator;
    use c8y_http_proxy::handle::C8YHttpProxy;
    use c8y_http_proxy::messages::C8YRestRequest;
    use c8y_http_proxy::messages::C8YRestResult;
    use rand::prelude::Distribution;
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    use serde_json::json;
    use std::collections::HashMap;
    use std::str::FromStr;
    use tedge_actors::Builder;
    use tedge_actors::LoggingSender;
    use tedge_actors::MessageReceiver;
    use tedge_actors::Sender;
    use tedge_actors::SimpleMessageBox;
    use tedge_actors::SimpleMessageBoxBuilder;
    use tedge_api::entity_store::EntityRegistrationMessage;
    use tedge_api::entity_store::EntityType;
    use tedge_api::entity_store::InvalidExternalIdError;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_mqtt_ext::test_helpers::assert_messages_matching;
    use tedge_mqtt_ext::Message;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;
    use tedge_utils::size_threshold::SizeThresholdExceededError;
    use test_case::test_case;

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

        let alarm_topic = "te/device/main///a/temperature_alarm";
        let alarm_payload = r#"{ "severity": "critical", "text": "Temperature very high" }"#;
        let alarm_message = Message::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

        // During the sync phase, alarms are not converted immediately, but only cached to be synced later
        assert!(converter.convert(&alarm_message).await.is_empty());

        let non_alarm_topic = "te/device/main///m/";
        let non_alarm_payload = r#"{"temp": 1}"#;
        let non_alarm_message =
            Message::new(&Topic::new_unchecked(non_alarm_topic), non_alarm_payload);

        // But non-alarms are converted immediately, even during the sync phase
        assert!(!converter.convert(&non_alarm_message).await.is_empty());

        let internal_alarm_topic = "c8y-internal/alarms/te/device/main///a/pressure_alarm";
        let internal_alarm_payload = r#"{ "severity": "major", "text": "Temperature very high" }"#;
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
            "te/device/main///a/pressure_alarm"
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

        let alarm_topic = "te/device/external_sensor///a/temperature_alarm";
        let alarm_payload = r#"{ "severity": "critical", "text": "Temperature very high" }"#;
        let alarm_message = Message::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

        // Child device creation messages are published.
        let device_creation_msgs = converter.convert(&alarm_message).await;
        assert_eq!(
            device_creation_msgs[0].topic.name,
            "te/device/external_sensor//"
        );
        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(
                device_creation_msgs[0].payload_str().unwrap()
            )
            .unwrap(),
            json!({
                "@type":"child-device",
                "@id":"test-device:device:external_sensor",
                "name": "external_sensor"
            })
        );

        let second_msg = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:external_sensor,external_sensor,thin-edge.io-child",
        );
        assert_eq!(device_creation_msgs[1], second_msg);

        // During the sync phase, alarms are not converted immediately, but only cached to be synced later
        assert!(converter.convert(&alarm_message).await.is_empty());

        let non_alarm_topic = "te/device/external_sensor///m/";
        let non_alarm_payload = r#"{"temp": 1}"#;
        let non_alarm_message =
            Message::new(&Topic::new_unchecked(non_alarm_topic), non_alarm_payload);

        // But non-alarms are converted immediately, even during the sync phase
        assert!(!converter.convert(&non_alarm_message).await.is_empty());

        let internal_alarm_topic =
            "c8y-internal/alarms/te/device/external_sensor///a/pressure_alarm";
        let internal_alarm_payload = r#"{ "severity": "major", "text": "Temperature very high" }"#;
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
            "te/device/external_sensor///a/pressure_alarm"
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

        let in_message = Message::new(
            &Topic::new_unchecked("te/device/child1///m/"),
            json!({
                "temp": 1,
                "time": "2021-11-16T17:45:40.571760714+01:00"
            })
            .to_string(),
        );

        let messages = converter.convert(&in_message).await;

        assert_messages_matching(
            &messages,
            [
                (
                    "te/device/child1//",
                    json!({
                        "@type":"child-device",
                        "@id":"test-device:device:child1",
                        "name":"child1"
                    })
                    .into(),
                ),
                (
                    "c8y/s/us",
                    "101,test-device:device:child1,child1,thin-edge.io-child".into(),
                ),
                (
                    "c8y/measurement/measurements/create",
                    json!({
                        "externalSource":{
                            "externalId":"test-device:device:child1",
                            "type":"c8y_Serial"
                        },
                        "temp":{
                            "temp":{
                                "value":1.0
                            }
                        },
                        "time":"2021-11-16T17:45:40.571760714+01:00",
                        "type":"ThinEdgeMeasurement"
                    })
                    .into(),
                ),
            ],
        );
    }

    #[tokio::test]
    async fn convert_measurement_with_nested_child_device() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let reg_message = Message::new(
            &Topic::new_unchecked("te/device/immediate_child//"),
            json!({
                "@type":"child-device",
                "@parent":"device/main//",
                "@id":"immediate_child"
            })
            .to_string(),
        );
        let _ = converter.convert(&reg_message).await;

        let reg_message = Message::new(
            &Topic::new_unchecked("te/device/nested_child//"),
            json!({
                "@type":"child-device",
                "@parent":"device/immediate_child//",
                "@id":"nested_child"
            })
            .to_string(),
        );
        let _ = converter.convert(&reg_message).await;

        let in_topic = "te/device/nested_child///m/";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            json!({
                "externalSource":{"externalId":"nested_child","type":"c8y_Serial"},
                "temp":{"temp":{"value":1.0}},
                "time":"2021-11-16T17:45:40.571760714+01:00",
                "type":"ThinEdgeMeasurement"
            })
            .to_string(),
        );

        // Test the first output messages contains SmartREST and C8Y JSON.
        let out_first_messages = converter.convert(&in_message).await;
        assert_eq!(out_first_messages, vec![expected_c8y_json_message.clone()]);
    }

    #[tokio::test]
    async fn convert_measurement_with_nested_child_service() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let reg_message = Message::new(
            &Topic::new_unchecked("te/device/immediate_child//"),
            json!({
                "@type":"child-device",
                "@parent":"device/main//",
                "@id":"immediate_child"
            })
            .to_string(),
        );
        let _ = converter.convert(&reg_message).await;

        let reg_message = Message::new(
            &Topic::new_unchecked("te/device/nested_child//"),
            json!({
                "@type":"child-device",
                "@parent":"device/immediate_child//",
                "@id":"nested_child"
            })
            .to_string(),
        );
        let _ = converter.convert(&reg_message).await;

        let reg_message = Message::new(
            &Topic::new_unchecked("te/device/nested_child/service/nested_service"),
            json!({
                "@type":"service",
                "@parent":"device/nested_child//",
                "@id":"nested_service"
            })
            .to_string(),
        );
        let _ = converter.convert(&reg_message).await;

        let in_topic = "te/device/nested_child/service/nested_service/m/";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            json!({
                "externalSource":{"externalId":"nested_service","type":"c8y_Serial"},
                "temp":{"temp":{"value":1.0}},
                "time":"2021-11-16T17:45:40.571760714+01:00",
                "type":"ThinEdgeMeasurement"
            })
            .to_string(),
        );

        // Test the first output messages contains SmartREST and C8Y JSON.
        let out_first_messages = converter.convert(&in_message).await;
        assert_eq!(out_first_messages, vec![expected_c8y_json_message.clone()]);
    }

    #[tokio::test]
    async fn convert_measurement_for_child_device_service() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "te/device/child1/service/app1/m/m_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_child_create_msg = Message::new(
            &Topic::new_unchecked("te/device/child1//"),
            json!({
                "@id":"test-device:device:child1",
                "@type":"child-device",
                "name":"child1",
            })
            .to_string(),
        )
        .with_retain();

        let expected_smart_rest_message_child = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:child1,child1,thin-edge.io-child",
        );
        let expected_service_create_msg = Message::new(
            &Topic::new_unchecked("te/device/child1/service/app1"),
            json!({
                "@id":"test-device:device:child1:service:app1",
                "@parent":"device/child1//",
                "@type":"service",
                "name":"app1",
                "type":"service"
            })
            .to_string(),
        )
        .with_retain();

        let expected_smart_rest_message_service = Message::new(
            &Topic::new_unchecked("c8y/s/us/test-device:device:child1"),
            "102,test-device:device:child1:service:app1,service,app1,up",
        );
        let expected_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            json!({
                "externalSource":{
                    "externalId":"test-device:device:child1:service:app1",
                    "type":"c8y_Serial"
                },
                "temp":{"temp":{"value":1.0}},
                "time":"2021-11-16T17:45:40.571760714+01:00",
                "type":"m_type"})
            .to_string(),
        );

        // Test the first output messages contains SmartREST and C8Y JSON.
        let out_first_messages = converter.convert(&in_message).await;
        assert_eq!(
            out_first_messages,
            vec![
                expected_child_create_msg,
                expected_smart_rest_message_child,
                expected_service_create_msg,
                expected_smart_rest_message_service,
                expected_c8y_json_message.clone()
            ]
        );

        // Test the second output messages doesn't contain SmartREST child device creation.
        let out_second_messages = converter.convert(&in_message).await;
        assert_eq!(out_second_messages, vec![expected_c8y_json_message]);
    }

    #[tokio::test]
    async fn convert_measurement_for_main_device_service() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "te/device/main/service/appm/m/m_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_create_service_msg = Message::new(
            &Topic::new_unchecked("te/device/main/service/appm"),
            json!({
                "@id":"test-device:device:main:service:appm",
                "@parent":"device/main//",
                "@type":"service",
                "name":"appm",
                "type":"service"})
            .to_string(),
        )
        .with_retain();

        let expected_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            json!({
                "externalSource":{
                    "externalId":"test-device:device:main:service:appm",
                    "type":"c8y_Serial"
                },
                "temp":{"temp":{"value":1.0}},
                "time":"2021-11-16T17:45:40.571760714+01:00",
                "type":"m_type"})
            .to_string(),
        );

        let expected_smart_rest_message_service = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "102,test-device:device:main:service:appm,service,appm,up",
        );

        // Test the first output messages contains SmartREST and C8Y JSON.
        let out_first_messages = converter.convert(&in_message).await;
        assert_eq!(
            out_first_messages,
            vec![
                expected_create_service_msg,
                expected_smart_rest_message_service,
                expected_c8y_json_message.clone()
            ]
        );

        let out_second_messages = converter.convert(&in_message).await;
        assert_eq!(out_second_messages, vec![expected_c8y_json_message]);
    }

    #[tokio::test]
    #[ignore = "FIXME: the registration is currently done even if the message is ill-formed"]
    async fn convert_first_measurement_invalid_then_valid_with_child_id() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "te/device/child1///m/";
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
        let out_second_messages: Vec<_> = converter
            .convert(&in_second_message)
            .await
            .into_iter()
            .filter(|m| m.topic.name.starts_with("c8y"))
            .collect();
        let expected_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:child1,child1,thin-edge.io-child",
        );
        let expected_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"externalSource":{"externalId":"test-device:device:child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
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
        let in_first_message =
            Message::new(&Topic::new_unchecked("te/device/child1///m/"), in_payload);
        let out_first_messages: Vec<_> = converter
            .convert(&in_first_message)
            .await
            .into_iter()
            .filter(|m| m.topic.name.starts_with("c8y"))
            .collect();
        let expected_first_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:child1,child1,thin-edge.io-child",
        );
        let expected_first_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"externalSource":{"externalId":"test-device:device:child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
        );
        assert_eq!(
            out_first_messages,
            vec![
                expected_first_smart_rest_message,
                expected_first_c8y_json_message
            ]
        );

        // Second message from "child2"
        let in_second_message =
            Message::new(&Topic::new_unchecked("te/device/child2///m/"), in_payload);
        let out_second_messages: Vec<_> = converter
            .convert(&in_second_message)
            .await
            .into_iter()
            .filter(|m| m.topic.name.starts_with("c8y"))
            .collect();
        let expected_second_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:child2,child2,thin-edge.io-child",
        );
        let expected_second_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"externalSource":{"externalId":"test-device:device:child2","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
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
    async fn convert_measurement_with_main_id_with_measurement_type() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "te/device/main///m/test_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"test_type"}"#,
        );

        // Test the output messages contains SmartREST and C8Y JSON.
        let out_first_messages: Vec<_> = converter
            .convert(&in_message)
            .await
            .into_iter()
            .filter(|m| m.topic.name.starts_with("c8y"))
            .collect();
        assert_eq!(out_first_messages, vec![expected_c8y_json_message.clone()]);
    }

    #[tokio::test]
    async fn convert_measurement_with_main_id_with_measurement_type_in_payload() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "te/device/main///m/test_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00","type":"type_in_payload"}"#;
        let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"type_in_payload"}"#,
        );

        // Test the output messages contains SmartREST and C8Y JSON.
        let out_messages: Vec<_> = converter
            .convert(&in_message)
            .await
            .into_iter()
            .filter(|m| m.topic.name.starts_with("c8y"))
            .collect();
        assert_eq!(out_messages, vec![expected_c8y_json_message.clone()]);
    }

    #[tokio::test]
    async fn convert_measurement_with_child_id_with_measurement_type() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "te/device/child///m/test_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:child,child,thin-edge.io-child",
        );

        let expected_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"externalSource":{"externalId":"test-device:device:child","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"test_type"}"#,
        );

        // Test the output messages contains SmartREST and C8Y JSON.
        let out_messages: Vec<_> = converter
            .convert(&in_message)
            .await
            .into_iter()
            .filter(|m| m.topic.name.starts_with("c8y"))
            .collect();
        assert_eq!(
            out_messages,
            vec![
                expected_smart_rest_message,
                expected_c8y_json_message.clone()
            ]
        );
    }

    #[tokio::test]
    async fn convert_measurement_with_child_id_with_measurement_type_in_payload() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "te/device/child2///m/test_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00","type":"type_in_payload"}"#;
        let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);
        let expected_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:child2,child2,thin-edge.io-child",
        );

        let expected_c8y_json_message = Message::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"externalSource":{"externalId":"test-device:device:child2","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"type_in_payload"}"#,
        );

        // Test the first output messages contains SmartREST and C8Y JSON.
        let out_first_messages: Vec<_> = converter
            .convert(&in_message)
            .await
            .into_iter()
            .filter(|m| m.topic.name.starts_with("c8y"))
            .collect();
        assert_eq!(
            out_first_messages,
            vec![
                expected_smart_rest_message,
                expected_c8y_json_message.clone()
            ]
        );
    }

    #[tokio::test]
    async fn check_c8y_threshold_packet_size() -> Result<(), anyhow::Error> {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let alarm_topic = "te/device/main///a/temperature_alarm";
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
    async fn convert_event_without_given_event_type() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let event_topic = "te/device/main///e/";
        let event_payload = r#"{ "text": "Someone clicked", "time": "2020-02-02T01:02:03+05:30" }"#;
        let event_message = Message::new(&Topic::new_unchecked(event_topic), event_payload);

        let converted_events = converter.convert(&event_message).await;
        assert_eq!(converted_events.len(), 1);
        let converted_event = converted_events.get(0).unwrap();
        assert_eq!(converted_event.topic.name, "c8y/s/us");

        assert_eq!(
            converted_event.payload_str().unwrap(),
            r#"400,ThinEdgeEvent,"Someone clicked",2020-02-02T01:02:03+05:30"#
        );
    }

    #[tokio::test]
    async fn convert_event_use_event_type_from_payload_to_c8y_smartrest() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let event_topic = "te/device/main///e/topic_event";
        let event_payload = r#"{ "type": "payload event", "text": "Someone clicked", "time": "2020-02-02T01:02:03+05:30" }"#;
        let event_message = Message::new(&Topic::new_unchecked(event_topic), event_payload);

        let converted_events = converter.convert(&event_message).await;
        assert_eq!(converted_events.len(), 1);
        let converted_event = converted_events.get(0).unwrap();
        assert_eq!(converted_event.topic.name, "c8y/s/us");

        assert_eq!(
            converted_event.payload_str().unwrap(),
            r#"400,payload event,"Someone clicked",2020-02-02T01:02:03+05:30"#
        );
    }

    #[tokio::test]
    async fn convert_event_use_event_type_from_payload_to_c8y_json() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let event_topic = "te/device/main///e/click_event";
        let event_payload = r#"{ "type": "payload event", "text": "tick", "foo": "bar" }"#;
        let event_message = Message::new(&Topic::new_unchecked(event_topic), event_payload);

        let converted_events = converter.convert(&event_message).await;
        assert_eq!(converted_events.len(), 1);

        let converted_event = converted_events.get(0).unwrap();
        let converted_c8y_json = json!({
            "type": "payload event",
            "text": "tick",
            "foo": "bar",
        });
        assert_eq!(converted_event.topic.name, "c8y/event/events/create");
        assert_json_include!(
            actual: serde_json::from_str::<serde_json::Value>(converted_event.payload_str().unwrap()).unwrap(),
            expected: converted_c8y_json
        );
    }

    #[tokio::test]
    async fn convert_event_with_known_fields_to_c8y_smartrest() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let event_topic = "te/device/main///e/click_event";
        let event_payload = r#"{ "text": "Someone clicked", "time": "2020-02-02T01:02:03+05:30" }"#;
        let event_message = Message::new(&Topic::new_unchecked(event_topic), event_payload);

        let converted_events = converter.convert(&event_message).await;
        assert_eq!(converted_events.len(), 1);
        let converted_event = converted_events.get(0).unwrap();
        assert_eq!(converted_event.topic.name, "c8y/s/us");

        assert_eq!(
            converted_event.payload_str().unwrap(),
            r#"400,click_event,"Someone clicked",2020-02-02T01:02:03+05:30"#
        );
    }

    #[tokio::test]
    async fn convert_event_with_extra_fields_to_c8y_json() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let event_topic = "te/device/main///e/click_event";
        let event_payload = r#"{ "text": "tick", "foo": "bar" }"#;
        let event_message = Message::new(&Topic::new_unchecked(event_topic), event_payload);

        let converted_events = converter.convert(&event_message).await;
        assert_eq!(converted_events.len(), 1);

        let converted_event = converted_events.get(0).unwrap();
        let converted_c8y_json = json!({
            "type": "click_event",
            "text": "tick",
            "foo": "bar",
        });
        assert_eq!(converted_event.topic.name, "c8y/event/events/create");
        assert_json_include!(
            actual: serde_json::from_str::<serde_json::Value>(converted_event.payload_str().unwrap()).unwrap(),
            expected: converted_c8y_json
        );
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

        let event_topic = "te/device/main///e/click_event";
        let big_event_text = create_packet((16 + 1) * 1024); // Event payload > size_threshold
        let big_event_payload = json!({ "text": big_event_text }).to_string();
        let big_event_message = Message::new(&Topic::new_unchecked(event_topic), big_event_payload);

        assert!(converter.convert(&big_event_message).await.is_empty());
    }

    #[tokio::test]
    async fn test_convert_big_measurement() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let measurement_topic = "te/device/main///m/";
        let big_measurement_payload = create_thin_edge_measurement(10 * 1024); // Measurement payload > size_threshold after converting to c8y json

        let big_measurement_message = Message::new(
            &Topic::new_unchecked(measurement_topic),
            big_measurement_payload,
        );
        let result = converter.convert(&big_measurement_message).await;

        let payload = result[0].payload_str().unwrap();
        assert!(payload.starts_with(
        r#"The payload {"temperature0":0,"temperature1":1,"temperature10" received on te/device/main///m/ after translation is"#
    ));
        assert!(payload.ends_with("greater than the threshold size of 16184."));
    }

    #[tokio::test]
    async fn test_convert_small_measurement() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let measurement_topic = "te/device/main///m/";
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
        let measurement_topic = "te/device/child1///m/";
        let big_measurement_payload = create_thin_edge_measurement(10 * 1024); // Measurement payload > size_threshold after converting to c8y json

        let big_measurement_message = Message::new(
            &Topic::new_unchecked(measurement_topic),
            big_measurement_payload,
        );

        let result = converter.convert(&big_measurement_message).await;

        let payload = result[0].payload_str().unwrap();
        assert!(payload.starts_with(
        r#"The payload {"temperature0":0,"temperature1":1,"temperature10" received on te/device/child1///m/ after translation is"#
    ));
        assert!(payload.ends_with("greater than the threshold size of 16184."));
    }

    #[tokio::test]
    async fn test_convert_small_measurement_for_child_device() {
        let tmp_dir = TempTedgeDir::new();
        let measurement_topic = "te/device/child1///m/";
        let big_measurement_payload = create_thin_edge_measurement(20); // Measurement payload size is 20 bytes

        let big_measurement_message = Message::new(
            &Topic::new_unchecked(measurement_topic),
            big_measurement_payload,
        );
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let result: Vec<_> = converter
            .convert(&big_measurement_message)
            .await
            .into_iter()
            .filter(|m| m.topic.name.starts_with("c8y"))
            .collect();

        let payload1 = &result[0].payload_str().unwrap();
        let payload2 = &result[1].payload_str().unwrap();

        assert!(payload1.contains("101,test-device:device:child1,child1,thin-edge.io-child"));
        assert!(payload2 .contains(
        r#"{"externalSource":{"externalId":"test-device:device:child1","type":"c8y_Serial"},"temperature0":{"temperature0":{"value":0.0}},"#
    ));
        assert!(payload2.contains(r#""type":"ThinEdgeMeasurement""#));
    }

    #[tokio::test]
    async fn translate_service_monitor_message_for_child_device() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "te/device/child1/service/child-service-c8y/status/health";
        let in_payload = r#"{"pid":"1234","status":"up","time":"2021-11-16T17:45:40.571760714+01:00","type":"thin-edge.io"}"#;
        let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

        let mqtt_schema = MqttSchema::new();
        let (in_entity, _in_channel) = mqtt_schema.entity_channel_of(&in_message.topic).unwrap();

        let expected_child_create_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:child1,child1,thin-edge.io-child",
        );

        let expected_service_monitor_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us/test-device:device:child1"),
            r#"102,test-device:device:child1:service:child-service-c8y,service,child-service-c8y,up"#,
        );

        let out_messages = converter.convert(&in_message).await;
        let mut out_messages = out_messages.into_iter();

        // child device entity store registration message
        let device_registration_message = out_messages.next().unwrap();
        let device_registration_message =
            EntityRegistrationMessage::new(&device_registration_message).unwrap();
        assert_eq!(
            device_registration_message.topic_id,
            in_entity.default_parent_identifier().unwrap()
        );
        assert_eq!(device_registration_message.r#type, EntityType::ChildDevice);

        // child device cloud registration message
        assert_eq!(
            out_messages.next().unwrap(),
            expected_child_create_smart_rest_message
        );

        // service entity store registration message
        let service_registration_message = out_messages.next().unwrap();
        let service_registration_message =
            EntityRegistrationMessage::new(&service_registration_message).unwrap();
        assert_eq!(service_registration_message.topic_id, in_entity);
        assert_eq!(service_registration_message.r#type, EntityType::Service);

        // service cloud registration message

        assert_eq!(
            out_messages.next().unwrap(),
            expected_service_monitor_smart_rest_message.clone()
        );
    }

    #[tokio::test]
    async fn translate_service_monitor_message_for_thin_edge_device() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "te/device/main/service/test-tedge-mapper-c8y/status/health";
        let in_payload = r#"{"pid":"1234","status":"up","time":"2021-11-16T17:45:40.571760714+01:00","type":"thin-edge.io"}"#;
        let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

        let mqtt_schema = MqttSchema::new();
        let (in_entity, _in_channel) = mqtt_schema.entity_channel_of(&in_message.topic).unwrap();

        let expected_service_monitor_smart_rest_message = Message::new(
            &Topic::new_unchecked("c8y/s/us"),
            r#"102,test-device:device:main:service:test-tedge-mapper-c8y,service,test-tedge-mapper-c8y,up"#,
        );

        // Test the output messages contains SmartREST and C8Y JSON.
        let mut out_messages = converter.convert(&in_message).await.into_iter();

        // service entity store registration message
        let service_registration_message = out_messages.next().unwrap();
        let service_registration_message =
            EntityRegistrationMessage::new(&service_registration_message).unwrap();
        assert_eq!(service_registration_message.topic_id, in_entity);
        assert_eq!(service_registration_message.r#type, EntityType::Service);

        let service_monitor_message = out_messages.next().unwrap();

        assert_eq!(
            service_monitor_message,
            expected_service_monitor_smart_rest_message
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

    #[test_case("device/main//", "test-device")]
    #[test_case(
        "device/main/service/tedge-agent",
        "test-device:device:main:service:tedge-agent"
    )]
    #[test_case("device/child1//", "test-device:device:child1")]
    #[test_case(
        "device/child1/service/collectd",
        "test-device:device:child1:service:collectd"
    )]
    #[test_case("custom_name///", "test-device:custom_name")]
    #[tokio::test]
    async fn entity_topic_id_to_c8y_external_id_mapping(
        entity_topic_id: &str,
        c8y_external_id: &str,
    ) {
        let entity_topic_id = EntityTopicId::from_str(entity_topic_id).unwrap();
        assert_eq!(
            CumulocityConverter::map_to_c8y_external_id(&entity_topic_id, &"test-device".into()),
            c8y_external_id.into()
        );
    }

    #[test_case("bad+name1", '+')]
    #[test_case("bad/name2", '/')]
    #[test_case("bad#name3", '#')]
    #[test_case("my/very#bad+name", '/')]
    fn sanitize_c8y_external_id(input_id: &str, invalid_char: char) {
        assert_eq!(
            CumulocityConverter::validate_external_id(input_id),
            Err(InvalidExternalIdError {
                external_id: input_id.into(),
                invalid_char
            })
        );
    }

    #[test_case("test-device:device:main", "main")]
    #[test_case("test-device:device:child", "child")]
    #[test_case("test-device:device:child:service:foo", "child")]
    #[test_case("test-device:device:child:foo:bar", "test-device:device:child:foo:bar")]
    #[test_case("a:very:long:and:complex:name", "a:very:long:and:complex:name")]
    #[test_case("non_default_name", "non_default_name")]
    #[tokio::test]
    async fn default_device_name_from_external_id(external_id: &str, device_name: &str) {
        let tmp_dir = TempTedgeDir::new();
        let (converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        assert_eq!(
            converter.default_device_name_from_external_id(&external_id.into()),
            device_name
        );
    }

    #[tokio::test]
    async fn duplicate_registration_messages_not_mapped_2311() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let measurement_message = Message::new(
            &Topic::new_unchecked("te/device/main/service/my_measurement_service/m/my_type"),
            r#"{"temperature": 21.37}"#,
        );

        // when auto-registered, local and cloud registration messages should be produced
        let mapped_messages = converter.convert(&measurement_message).await;

        let local_registration_message = mapped_messages
            .iter()
            .find(|m| EntityRegistrationMessage::new(m).is_some())
            .unwrap();

        // check if cloud registration message
        assert!(mapped_messages
            .iter()
            .any(|m| m.topic.name == "c8y/s/us" && m.payload_str().unwrap().starts_with("102")));

        // when converting a registration message the same as the previous one, no additional registration messages should be produced
        let mapped_messages = converter.convert(local_registration_message).await;

        let second_registration_message_mapped = mapped_messages.into_iter().any(|m| {
            m.topic.name.starts_with("c8y/s/us") && m.payload_str().unwrap().starts_with("102")
        });
        assert!(!second_registration_message_mapped);
    }

    pub(crate) async fn create_c8y_converter(
        tmp_dir: &TempTedgeDir,
    ) -> (
        CumulocityConverter,
        SimpleMessageBox<C8YRestRequest, C8YRestResult>,
    ) {
        tmp_dir.dir("operations").dir("c8y");
        tmp_dir.dir("tedge").dir("agent");

        let device_id = "test-device".into();
        let device_topic_id = EntityTopicId::default_main_device();
        let device_type = "test-device-type".into();
        let service_type = "service".into();
        let c8y_host = "test.c8y.io".into();
        let tedge_http_host = "localhost".into();
        let mqtt_schema = MqttSchema::default();
        let auth_proxy_addr = [127, 0, 0, 1].into();
        let auth_proxy_port = 8001;
        let mut topics =
            C8yMapperConfig::default_internal_topic_filter(&tmp_dir.to_path_buf()).unwrap();
        topics.add_all(crate::log_upload::log_upload_topic_filter(&mqtt_schema));
        topics.add_all(C8yMapperConfig::default_external_topic_filter());

        let config = C8yMapperConfig::new(
            tmp_dir.to_path_buf(),
            tmp_dir.utf8_path_buf(),
            tmp_dir.utf8_path_buf().into(),
            device_id,
            device_topic_id,
            device_type,
            service_type,
            c8y_host,
            tedge_http_host,
            topics,
            Capabilities::default(),
            auth_proxy_addr,
            auth_proxy_port,
        );

        let mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
            SimpleMessageBoxBuilder::new("MQTT", 5);
        let mqtt_publisher = LoggingSender::new("MQTT".into(), mqtt_builder.build().sender_clone());

        let mut c8y_proxy_builder: SimpleMessageBoxBuilder<C8YRestRequest, C8YRestResult> =
            SimpleMessageBoxBuilder::new("C8Y", 1);
        let http_proxy = C8YHttpProxy::new("C8Y", &mut c8y_proxy_builder);
        let auth_proxy = ProxyUrlGenerator::new(auth_proxy_addr, auth_proxy_port);

        let downloader_builder: SimpleMessageBoxBuilder<IdDownloadResult, IdDownloadRequest> =
            SimpleMessageBoxBuilder::new("MQTT", 5);
        let downloader_sender =
            LoggingSender::new("DL".into(), downloader_builder.build().sender_clone());

        let converter = CumulocityConverter::new(
            config,
            mqtt_publisher,
            http_proxy,
            auth_proxy,
            downloader_sender,
        )
        .unwrap();

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
