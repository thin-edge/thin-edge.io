use super::alarm_converter::AlarmConverter;
use super::config::C8yMapperConfig;
use super::config::MQTT_MESSAGE_SIZE_THRESHOLD;
use super::error::CumulocityMapperError;
use super::service_monitor;
use crate::actor::CmdId;
use crate::actor::IdDownloadRequest;
use crate::actor::IdUploadRequest;
use crate::dynamic_discovery::DiscoverOp;
use crate::error::ConversionError;
use crate::json;
use crate::operations::FtsDownloadOperationData;
use anyhow::anyhow;
use anyhow::Context;
use c8y_api::http_proxy::C8yEndPoint;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::json_c8y::C8yUpdateSoftwareListResponse;
use c8y_api::json_c8y_deserializer::C8yDeviceControlOperation;
use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
use c8y_api::json_c8y_deserializer::C8yJsonOverMqttDeserializerError;
use c8y_api::json_c8y_deserializer::C8yOperation;
use c8y_api::json_c8y_deserializer::C8ySoftwareUpdate;
use c8y_api::smartrest::error::OperationsError;
use c8y_api::smartrest::error::SmartRestDeserializerError;
use c8y_api::smartrest::inventory::child_device_creation_message;
use c8y_api::smartrest::inventory::service_creation_message;
use c8y_api::smartrest::message::collect_smartrest_messages;
use c8y_api::smartrest::message::get_failure_reason_for_smartrest;
use c8y_api::smartrest::message::get_smartrest_device_id;
use c8y_api::smartrest::message::get_smartrest_template_id;
use c8y_api::smartrest::message::sanitize_bytes_for_smartrest;
use c8y_api::smartrest::message::MAX_PAYLOAD_LIMIT_IN_BYTES;
use c8y_api::smartrest::operations::get_child_ops;
use c8y_api::smartrest::operations::get_operations;
use c8y_api::smartrest::operations::Operations;
use c8y_api::smartrest::operations::ResultFormat;
use c8y_api::smartrest::smartrest_serializer::fail_operation;
use c8y_api::smartrest::smartrest_serializer::get_advanced_software_list_payloads;
use c8y_api::smartrest::smartrest_serializer::request_pending_operations;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing;
use c8y_api::smartrest::smartrest_serializer::succeed_operation;
use c8y_api::smartrest::smartrest_serializer::succeed_operation_no_payload;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::EmbeddedCsv;
use c8y_api::smartrest::smartrest_serializer::TextOrCsv;
use c8y_api::smartrest::topic::publish_topic_from_ancestors;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_auth_proxy::url::ProxyUrlGenerator;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::CreateEvent;
use camino::Utf8Path;
use logged_command::LoggedCommand;
use plugin_sm::operation_logs::OperationLogs;
use plugin_sm::operation_logs::OperationLogsError;
use serde_json::json;
use serde_json::Map;
use serde_json::Value;
use service_monitor::convert_health_status_message;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tedge_actors::LoggingSender;
use tedge_actors::Sender;
use tedge_api::commands::CommandStatus;
use tedge_api::commands::RestartCommand;
use tedge_api::commands::SoftwareCommandMetadata;
use tedge_api::commands::SoftwareListCommand;
use tedge_api::commands::SoftwareUpdateCommand;
use tedge_api::entity_store;
use tedge_api::entity_store::EntityExternalId;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::entity_store::EntityType;
use tedge_api::entity_store::Error;
use tedge_api::entity_store::InvalidExternalIdError;
use tedge_api::event::error::ThinEdgeJsonDeserializerError;
use tedge_api::event::ThinEdgeEvent;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::IdGenerator;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::pending_entity_store::PendingEntityData;
use tedge_api::workflow::GenericCommandState;
use tedge_api::DownloadInfo;
use tedge_api::EntityStore;
use tedge_api::Jsonify;
use tedge_config::AutoLogUpload;
use tedge_config::SoftwareManagementApiFlag;
use tedge_config::TEdgeConfigError;
use tedge_config::TopicPrefix;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_uploader_ext::ContentType;
use tedge_uploader_ext::FormData;
use tedge_uploader_ext::Mime;
use tedge_uploader_ext::UploadRequest;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::create_file_with_defaults;
use tedge_utils::file::FileError;
use tedge_utils::size_threshold::SizeThreshold;
use thiserror::Error;
use time::OffsetDateTime;
use tokio::time::Duration;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::trace;
use tracing::warn;
use url::Url;

const INTERNAL_ALARMS_TOPIC: &str = "c8y-internal/alarms/";
const C8Y_JSON_MQTT_EVENTS_TOPIC: &str = "event/events/create";
const TEDGE_AGENT_LOG_DIR: &str = "agent";
const CREATE_EVENT_SMARTREST_CODE: u16 = 400;
const DEFAULT_EVENT_TYPE: &str = "ThinEdgeEvent";
const FORBIDDEN_ID_CHARS: [char; 3] = ['/', '+', '#'];
const REQUESTER_NAME: &str = "c8y-mapper";
const EARLY_MESSAGE_BUFFER_SIZE: usize = 100;
const SOFTWARE_LIST_CHUNK_SIZE: usize = 100;

#[derive(Debug)]
pub struct MapperConfig {
    pub out_topic: Topic,
    pub errors_topic: Topic,
}

impl CumulocityConverter {
    pub async fn convert(&mut self, input: &MqttMessage) -> Vec<MqttMessage> {
        let messages_or_err = self.try_convert(input).await;
        self.wrap_errors(messages_or_err)
    }

    pub fn wrap_errors(
        &self,
        messages_or_err: Result<Vec<MqttMessage>, ConversionError>,
    ) -> Vec<MqttMessage> {
        messages_or_err.unwrap_or_else(|error| vec![self.new_error_message(error)])
    }

    pub fn wrap_error(&self, message_or_err: Result<MqttMessage, ConversionError>) -> MqttMessage {
        message_or_err.unwrap_or_else(|error| self.new_error_message(error))
    }

    pub fn new_error_message(&self, error: ConversionError) -> MqttMessage {
        error!("Mapping error: {}", error);
        MqttMessage::new(&self.get_mapper_config().errors_topic, error.to_string())
    }

    /// This function will be the first method that's called on the converter after it's instantiated.
    /// Return any initialization messages that must be processed before the converter starts converting regular messages.
    pub fn init_messages(&mut self) -> Vec<MqttMessage> {
        match self.try_init_messages() {
            Ok(messages) => messages,
            Err(error) => {
                error!("Mapping error: {}", error);
                vec![MqttMessage::new(
                    &self.get_mapper_config().errors_topic,
                    error.to_string(),
                )]
            }
        }
    }

    pub fn process_operation_update_message(&mut self, message: DiscoverOp) -> MqttMessage {
        let message_or_err = self.try_process_operation_update_message(&message);
        match message_or_err {
            Ok(Some(msg)) => msg,
            Ok(None) => MqttMessage::new(
                &self.get_mapper_config().errors_topic,
                "No operation update required",
            ),
            Err(err) => self.new_error_message(err),
        }
    }
}

pub struct UploadOperationData {
    pub topic_id: EntityTopicId,
    pub smartrest_topic: Topic,
    pub clear_cmd_topic: Topic,
    pub c8y_binary_url: String,
    pub operation: CumulocitySupportedOperations,
    pub command: GenericCommandState,

    // used to automatically remove the temporary file after operation is finished
    pub file_dir: tempfile::TempDir,
}

pub struct UploadOperationLog {
    pub final_messages: Vec<MqttMessage>,
}

pub enum UploadContext {
    OperationData(UploadOperationData),
    OperationLog(UploadOperationLog),
}

impl From<UploadOperationData> for UploadContext {
    fn from(value: UploadOperationData) -> Self {
        UploadContext::OperationData(value)
    }
}

impl From<UploadOperationLog> for UploadContext {
    fn from(value: UploadOperationLog) -> Self {
        UploadContext::OperationLog(value)
    }
}

pub struct CumulocityConverter {
    pub(crate) size_threshold: SizeThreshold,
    pub config: Arc<C8yMapperConfig>,
    pub(crate) mapper_config: MapperConfig,
    pub device_name: String,
    pub(crate) device_topic_id: EntityTopicId,
    pub(crate) device_type: String,
    alarm_converter: AlarmConverter,
    pub operations: Operations,
    operation_logs: OperationLogs,
    mqtt_publisher: LoggingSender<MqttMessage>,
    pub http_proxy: C8YHttpProxy,
    pub children: HashMap<String, Operations>,
    pub service_type: String,
    pub c8y_endpoint: C8yEndPoint,
    pub mqtt_schema: MqttSchema,
    pub entity_store: EntityStore,
    pub auth_proxy: ProxyUrlGenerator,
    pub uploader_sender: LoggingSender<IdUploadRequest>,
    pub downloader_sender: LoggingSender<IdDownloadRequest>,
    pub pending_upload_operations: HashMap<CmdId, UploadContext>,

    /// Used to store pending downloads from the FTS.
    // Using a separate field to not mix downloads from FTS and HTTP proxy
    pub pending_fts_download_operations: HashMap<CmdId, FtsDownloadOperationData>,

    pub command_id: IdGenerator,
    // Keep active command IDs to avoid creation of multiple commands for an operation
    pub active_commands: HashSet<CmdId>,
}

impl CumulocityConverter {
    pub fn new(
        config: C8yMapperConfig,
        mqtt_publisher: LoggingSender<MqttMessage>,
        http_proxy: C8YHttpProxy,
        auth_proxy: ProxyUrlGenerator,
        uploader_sender: LoggingSender<IdUploadRequest>,
        downloader_sender: LoggingSender<IdDownloadRequest>,
    ) -> Result<Self, CumulocityConverterBuildError> {
        let device_id = config.device_id.clone();
        let device_topic_id = config.device_topic_id.clone();
        let device_type = config.device_type.clone();

        let service_type = if config.service.ty.is_empty() {
            "service".to_owned()
        } else {
            config.service.ty.clone()
        };

        let c8y_host = config.c8y_host.clone();

        let size_threshold = SizeThreshold(MQTT_MESSAGE_SIZE_THRESHOLD);

        let operations = Operations::try_new(&*config.ops_dir)?;
        let children = get_child_ops(&*config.ops_dir)?;

        let alarm_converter = AlarmConverter::new();

        let log_dir = config.logs_path.join(TEDGE_AGENT_LOG_DIR);
        let operation_logs = OperationLogs::try_new(log_dir.into())?;

        let c8y_endpoint = C8yEndPoint::new(&c8y_host, &device_id);

        let mqtt_schema = config.mqtt_schema.clone();

        let prefix = &config.c8y_prefix;
        let mapper_config = MapperConfig {
            out_topic: Topic::new_unchecked(&format!("{prefix}/measurement/measurements/create")),
            errors_topic: mqtt_schema.error_topic(),
        };

        let main_device = entity_store::EntityRegistrationMessage::main_device(device_id.clone());
        let entity_store = EntityStore::with_main_device_and_default_service_type(
            mqtt_schema.clone(),
            main_device,
            service_type.clone(),
            Self::map_to_c8y_external_id,
            Self::validate_external_id,
            EARLY_MESSAGE_BUFFER_SIZE,
            &*config.state_dir,
            config.clean_start,
        )
        .unwrap();

        let command_id = IdGenerator::new(REQUESTER_NAME);

        Ok(CumulocityConverter {
            size_threshold,
            config: Arc::new(config),
            mapper_config,
            device_name: device_id,
            device_topic_id,
            device_type,
            alarm_converter,
            operations,
            operation_logs,
            http_proxy,
            children,
            mqtt_publisher,
            service_type,
            c8y_endpoint,
            mqtt_schema,
            entity_store,
            auth_proxy,
            uploader_sender,
            downloader_sender,
            pending_upload_operations: HashMap::new(),
            pending_fts_download_operations: HashMap::new(),
            command_id,
            active_commands: HashSet::new(),
        })
    }

    pub fn try_convert_entity_registration(
        &mut self,
        input: &EntityRegistrationMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        // Parse the optional fields
        let display_name = input.other.get("name").and_then(|v| v.as_str());
        let display_type = input.other.get("type").and_then(|v| v.as_str());

        let entity_topic_id = &input.topic_id;
        let external_id = self
            .entity_store
            .try_get(entity_topic_id)
            .map(|e| &e.external_id)?;
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
                    &self.config.c8y_prefix,
                )
                .context("Could not create device creation message")?;
                Ok(vec![child_creation_message])
            }
            EntityType::Service => {
                let ancestors_external_ids =
                    self.entity_store.ancestors_external_ids(entity_topic_id)?;

                if self.config.bridge_in_mapper && entity_topic_id.is_bridge_health_topic() {
                    // Skip service creation for the mapper-inbuilt bridge, as it is part of the mapper service itself
                    return Ok(vec![]);
                }

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
                    &self.config.c8y_prefix,
                )
                .context("Could not create service creation message")?;
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
        Ok(publish_topic_from_ancestors(
            &ancestors_external_ids,
            &self.config.c8y_prefix,
        ))
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
        input: &MqttMessage,
        measurement_type: &str,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut mqtt_messages: Vec<MqttMessage> = Vec::new();

        if let Some(entity) = self.entity_store.get(source) {
            // Need to check if the input Thin Edge JSON is valid before adding a child ID to list
            let c8y_json_payload =
                json::from_thin_edge_json(input.payload_str()?, entity, measurement_type)?;

            if c8y_json_payload.len() < self.size_threshold.0 {
                mqtt_messages.push(MqttMessage::new(
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
        }
        Ok(mqtt_messages)
    }

    async fn try_convert_event(
        &mut self,
        source: &EntityTopicId,
        input: &MqttMessage,
        event_type: &str,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
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
                let smartrest_topic = C8yTopic::upstream_topic(&self.config.c8y_prefix);
                MqttMessage::new(&smartrest_topic, smartrest_event)
            } else {
                // If the message contains extra fields other than `text` and `time`, convert to Cumulocity JSON
                let cumulocity_event_json = serde_json::to_string(&c8y_event)?;
                let json_mqtt_topic = Topic::new_unchecked(&format!(
                    "{}/{C8Y_JSON_MQTT_EVENTS_TOPIC}",
                    self.config.c8y_prefix
                ));
                MqttMessage::new(&json_mqtt_topic, cumulocity_event_json)
            };

            if self.can_send_over_mqtt(&message) {
                // The message can be sent via MQTT
                messages.push(message);
            } else {
                // The message must be sent over HTTP
                let create_event = CreateEvent {
                    event_type: c8y_event.event_type,
                    time: c8y_event.time,
                    text: c8y_event.text,
                    extras: c8y_event.extras,
                    device_id: entity.external_id.clone().into(),
                };
                self.http_proxy.send_event(create_event).await?;
                return Ok(vec![]);
            }
        }
        Ok(messages)
    }

    pub fn process_alarm_messages(
        &mut self,
        source: &EntityTopicId,
        input: &MqttMessage,
        alarm_type: &str,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        self.size_threshold.validate(input)?;

        let mqtt_messages = self.alarm_converter.try_convert_alarm(
            source,
            input,
            alarm_type,
            &self.entity_store,
            &self.config.c8y_prefix,
        )?;

        Ok(mqtt_messages)
    }

    pub async fn process_health_status_message(
        &mut self,
        entity: &EntityTopicId,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let entity_metadata = self
            .entity_store
            .get(entity)
            .expect("entity was registered");

        let ancestors_external_ids = self.entity_store.ancestors_external_ids(entity)?;
        Ok(convert_health_status_message(
            entity_metadata,
            &ancestors_external_ids,
            message,
            &self.config.c8y_prefix,
        ))
    }

    async fn parse_c8y_devicecontrol_topic(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let operation = C8yOperation::from_json(message.payload.as_str()?)?;
        let device_xid = operation.external_source.external_id;
        let cmd_id = self.command_id.new_id_with_str(&operation.op_id);

        if self.active_commands.contains(&cmd_id) {
            info!("{cmd_id} is already addressed");
            return Ok(vec![]);
        }

        let result = self
            .process_json_over_mqtt(device_xid, cmd_id, &operation.extras)
            .await;
        let output = self.handle_c8y_operation_result(&result);

        Ok(output)
    }

    async fn process_json_over_mqtt(
        &mut self,
        device_xid: String,
        cmd_id: String,
        extras: &HashMap<String, Value>,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let msgs = match C8yDeviceControlOperation::from_json_object(extras)? {
            C8yDeviceControlOperation::Restart(_) => {
                self.forward_restart_request(device_xid, cmd_id)?
            }
            C8yDeviceControlOperation::SoftwareUpdate(request) => {
                self.forward_software_request(device_xid, cmd_id, request)
                    .await?
            }
            C8yDeviceControlOperation::LogfileRequest(request) => {
                if self.config.capabilities.log_upload {
                    self.convert_log_upload_request(device_xid, cmd_id, request)?
                } else {
                    warn!("Received a c8y_LogfileRequest operation, however, log_upload feature is disabled");
                    vec![]
                }
            }
            C8yDeviceControlOperation::UploadConfigFile(request) => {
                if self.config.capabilities.config_snapshot {
                    self.convert_config_snapshot_request(device_xid, cmd_id, request)?
                } else {
                    warn!("Received a c8y_UploadConfigFile operation, however, config_snapshot feature is disabled");
                    vec![]
                }
            }
            C8yDeviceControlOperation::DownloadConfigFile(request) => {
                if self.config.capabilities.config_update {
                    self.convert_config_update_request(device_xid, cmd_id, request)
                        .await?
                } else {
                    warn!("Received a c8y_DownloadConfigFile operation, however, config_update feature is disabled");
                    vec![]
                }
            }
            C8yDeviceControlOperation::Firmware(request) => {
                if self.config.capabilities.firmware_update {
                    self.convert_firmware_update_request(device_xid, cmd_id, request)?
                } else {
                    warn!("Received a c8y_Firmware operation, however, firmware_update feature is disabled");
                    vec![]
                }
            }
            C8yDeviceControlOperation::Custom => {
                // Ignores custom and static template operations unsupported by thin-edge
                // However, these operations can be addressed by SmartREST that is published together with JSON over MQTT
                vec![]
            }
        };

        Ok(msgs)
    }

    async fn parse_c8y_smartrest_topics(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut output: Vec<MqttMessage> = Vec::new();
        for smartrest_message in collect_smartrest_messages(message.payload_str()?) {
            let result = self.process_smartrest(smartrest_message.as_str()).await;
            let mut msgs = self.handle_c8y_operation_result(&result);
            output.append(&mut msgs)
        }
        Ok(output)
    }

    fn handle_c8y_operation_result(
        &mut self,
        result: &Result<Vec<MqttMessage>, CumulocityMapperError>,
    ) -> Vec<MqttMessage> {
        match result {
            Err(
                err @ CumulocityMapperError::FromSmartRestDeserializer(
                    SmartRestDeserializerError::InvalidParameter { operation, .. },
                )
                | err @ CumulocityMapperError::FromC8yJsonOverMqttDeserializerError(
                    C8yJsonOverMqttDeserializerError::InvalidParameter { operation, .. },
                )
                | err @ CumulocityMapperError::ExecuteFailed {
                    operation_name: operation,
                    ..
                },
            ) => {
                let topic = C8yTopic::SmartRestResponse
                    .to_topic(&self.config.c8y_prefix)
                    .unwrap();
                let msg1 = MqttMessage::new(&topic, set_operation_executing(operation));
                let msg2 = MqttMessage::new(&topic, fail_operation(operation, &err.to_string()));
                error!("{err}");
                vec![msg1, msg2]
            }
            Err(err) => {
                error!("{err}");
                vec![]
            }

            Ok(msgs) => msgs.to_owned(),
        }
    }

    async fn process_smartrest(
        &mut self,
        payload: &str,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        match get_smartrest_device_id(payload) {
            Some(device_id) => {
                match get_smartrest_template_id(payload).as_str() {
                    template if device_id == self.device_name => {
                        self.forward_operation_request(payload, template).await
                    }
                    _ => {
                        // Ignore any other child device incoming request as not yet supported
                        debug!("Ignored. Message not yet supported: {payload}");
                        Ok(vec![])
                    }
                }
            }
            None => {
                debug!("Ignored. Message not yet supported: {payload}");
                Ok(vec![])
            }
        }
    }

    async fn forward_software_request(
        &mut self,
        device_xid: String,
        cmd_id: String,
        software_update_request: C8ySoftwareUpdate,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let entity_xid: EntityExternalId = device_xid.into();
        let target = self.entity_store.try_get_by_external_id(&entity_xid)?;
        let mut command =
            software_update_request.into_software_update_command(&target.topic_id, cmd_id)?;

        command.payload.update_list.iter_mut().for_each(|modules| {
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

        let message = command.command_message(&self.mqtt_schema);
        Ok(vec![message])
    }

    fn forward_restart_request(
        &mut self,
        device_xid: String,
        cmd_id: String,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let entity_xid: EntityExternalId = device_xid.into();
        let target = self.entity_store.try_get_by_external_id(&entity_xid)?;
        let command = RestartCommand::new(&target.topic_id, cmd_id);
        let message = command.command_message(&self.mqtt_schema);
        Ok(vec![message])
    }

    fn request_software_list(&self, target: &EntityTopicId) -> MqttMessage {
        let cmd_id = self.command_id.new_id();
        let request = SoftwareListCommand::new(target, cmd_id);
        request.command_message(&self.mqtt_schema)
    }

    async fn forward_operation_request(
        &mut self,
        payload: &str,
        template: &str,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        if let Some(operation) = self.operations.matching_smartrest_template(template) {
            if let Some(command) = operation.command() {
                self.execute_operation(
                    payload,
                    command.as_str(),
                    operation.result_format(),
                    operation.graceful_timeout(),
                    operation.forceful_timeout(),
                    operation.name,
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
        result_format: ResultFormat,
        graceful_timeout: Duration,
        forceful_timeout: Duration,
        operation_name: String,
    ) -> Result<(), CumulocityMapperError> {
        let command = command.to_owned();
        let payload = payload.to_string();

        let mut logged =
            LoggedCommand::new(&command).map_err(|e| CumulocityMapperError::ExecuteFailed {
                error_message: e.to_string(),
                command: command.to_string(),
                operation_name: operation_name.to_string(),
            })?;

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
                let op_name = operation_name.to_owned();
                let mut mqtt_publisher = self.mqtt_publisher.clone();
                let c8y_prefix = self.config.c8y_prefix.clone();

                tokio::spawn(async move {
                    let op_name = op_name.as_str();
                    let logger = log_file.buffer();

                    // mqtt client publishes executing
                    let topic = C8yTopic::SmartRestResponse.to_topic(&c8y_prefix).unwrap();
                    let executing_str = set_operation_executing(op_name);
                    mqtt_publisher
                        .send(MqttMessage::new(&topic, executing_str.as_str()))
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
                                let sanitized_stdout = sanitize_bytes_for_smartrest(
                                    &output.stdout,
                                    MAX_PAYLOAD_LIMIT_IN_BYTES,
                                );
                                let result = match result_format {
                                    ResultFormat::Text => TextOrCsv::Text(sanitized_stdout),
                                    ResultFormat::Csv => EmbeddedCsv::new(sanitized_stdout).into(),
                                };
                                let success_message = succeed_operation(op_name, result);
                                match success_message {
                                    Ok(message) => mqtt_publisher.send(MqttMessage::new(&topic, message.as_str())).await
                                        .unwrap_or_else(|err| {
                                            error!("Failed to publish a message: {message}. Error: {err}")
                                        }),
                                    Err(e) => {
                                        let fail_message = fail_operation(
                                            op_name,
                                            &format!("{:?}", anyhow::Error::from(e).context("Custom operation process exited successfully, but couldn't convert output to valid SmartREST message")));
                                        mqtt_publisher.send(MqttMessage::new(&topic, fail_message.as_str())).await.unwrap_or_else(|err| {
                                            error!("Failed to publish a message: {fail_message}. Error: {err}")
                                        })
                                    }
                                }
                            }
                            _ => {
                                let failure_reason = get_failure_reason_for_smartrest(
                                    &output.stderr,
                                    MAX_PAYLOAD_LIMIT_IN_BYTES,
                                );
                                let payload = fail_operation(op_name, &failure_reason);

                                mqtt_publisher
                                    .send(MqttMessage::new(&topic, payload.as_bytes()))
                                    .await
                                    .unwrap_or_else(|err| {
                                        error!(
                                            "Failed to publish a message: {payload}. Error: {err}"
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

    fn serialize_to_smartrest(c8y_event: &C8yCreateEvent) -> Result<String, ConversionError> {
        Ok(format!(
            "{},{},\"{}\",{}",
            CREATE_EVENT_SMARTREST_CODE,
            c8y_event.event_type,
            c8y_event.text,
            c8y_event
                .time
                .format(&time::format_description::well_known::Rfc3339)?
        ))
    }

    fn can_send_over_mqtt(&self, message: &MqttMessage) -> bool {
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

    #[error(transparent)]
    FileError(#[from] FileError),
}

impl CumulocityConverter {
    fn get_mapper_config(&self) -> &MapperConfig {
        &self.mapper_config
    }

    pub async fn try_convert(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        debug!("Mapping message on topic: {}", message.topic.name);
        trace!("Message content: {:?}", message.payload_str());
        match self.mqtt_schema.entity_channel_of(&message.topic) {
            Ok((source, channel)) => self.try_convert_te_topics(source, channel, message).await,
            Err(_) => self.try_convert_tedge_and_c8y_topics(message).await,
        }
    }

    async fn try_convert_te_topics(
        &mut self,
        source: EntityTopicId,
        channel: Channel,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        match &channel {
            Channel::EntityMetadata => {
                if let Ok(register_message) = EntityRegistrationMessage::try_from(message) {
                    return self
                        .try_register_entity_with_pending_children(register_message)
                        .await;
                }
                Err(anyhow!(
                    "Invalid entity registration message received on topic: {}",
                    message.topic.name
                )
                .into())
            }
            _ => {
                let mut converted_messages: Vec<MqttMessage> = vec![];
                // if the target entity is unregistered, try to register it first using auto-registration
                if self.entity_store.get(&source).is_none() {
                    // On receipt of an unregistered entity data message with custom topic scheme OR
                    // one with default topic scheme itself when auto registration disabled,
                    // since it is received before the entity itself is registered,
                    // cache it in the unregistered entity store to be processed after the entity is registered
                    if !(self.config.enable_auto_register && source.matches_default_topic_scheme())
                    {
                        self.entity_store.cache_early_data_message(message.clone());
                        return Ok(vec![]);
                    }

                    let auto_registered_entities = self.try_auto_register_entity(&source)?;
                    converted_messages =
                        self.try_convert_auto_registered_entities(auto_registered_entities)?;
                }

                let result = self
                    .try_convert_data_message(source, channel, message)
                    .await;
                let mut messages = self.wrap_errors(result);

                converted_messages.append(&mut messages);
                Ok(converted_messages)
            }
        }
    }

    async fn try_register_entity_with_pending_children(
        &mut self,
        register_message: EntityRegistrationMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut mapped_messages = vec![];
        match self.entity_store.update(register_message.clone()) {
            Err(e) => {
                error!("Entity registration failed: {e}");
            }
            Ok((affected_entities, pending_entities)) if !affected_entities.is_empty() => {
                for pending_entity in pending_entities {
                    // Register and convert the entity registration first
                    let mut c8y_message =
                        self.try_convert_entity_registration(&pending_entity.reg_message)?;
                    mapped_messages.append(&mut c8y_message);

                    // Process all the cached data messages for that entity
                    let mut cached_messages =
                        self.process_cached_entity_data(pending_entity).await?;
                    mapped_messages.append(&mut cached_messages);
                }

                return Ok(mapped_messages);
            }
            Ok(_) => {}
        }
        Ok(mapped_messages)
    }

    fn try_auto_register_entity(
        &mut self,
        source: &EntityTopicId,
    ) -> Result<Vec<EntityRegistrationMessage>, ConversionError> {
        let auto_registered_entities = self.entity_store.auto_register_entity(source)?;
        for auto_registered_entity in &auto_registered_entities {
            if auto_registered_entity.r#type == EntityType::ChildDevice {
                self.children.insert(
                    self.entity_store
                        .get(source)
                        .expect("Entity should have been auto registered in the previous step")
                        .external_id
                        .as_ref()
                        .into(),
                    Operations::default(),
                );
            }
        }
        Ok(auto_registered_entities)
    }

    fn try_convert_auto_registered_entities(
        &mut self,
        entities: Vec<EntityRegistrationMessage>,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut converted_messages: Vec<MqttMessage> = vec![];
        for entity in entities {
            // Append the entity registration message itself and its converted c8y form
            converted_messages.append(&mut self.try_convert_auto_registered_entity(&entity)?);
        }
        Ok(converted_messages)
    }

    async fn try_convert_data_message(
        &mut self,
        source: EntityTopicId,
        channel: Channel,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        match &channel {
            Channel::EntityTwinData { fragment_key } => {
                self.try_convert_entity_twin_data(&source, message, fragment_key)
            }

            Channel::Measurement { measurement_type } => {
                self.try_convert_measurement(&source, message, measurement_type)
            }

            Channel::Event { event_type } => {
                self.try_convert_event(&source, message, event_type).await
            }

            Channel::Alarm { alarm_type } => {
                self.process_alarm_messages(&source, message, alarm_type)
            }

            Channel::Command { cmd_id, .. } if message.payload_bytes().is_empty() => {
                // The command has been fully processed
                self.active_commands.remove(cmd_id);
                Ok(vec![])
            }

            Channel::CommandMetadata { operation } => {
                self.validate_operation_supported(operation, &source)?;
                match operation {
                    OperationType::Restart => self.register_restart_operation(&source).await,
                    OperationType::SoftwareList => {
                        self.register_software_list_operation(&source, message)
                            .await
                    }
                    OperationType::SoftwareUpdate => {
                        self.register_software_update_operation(&source).await
                    }
                    OperationType::LogUpload => self.convert_log_metadata(&source, message),
                    OperationType::ConfigSnapshot => {
                        self.convert_config_snapshot_metadata(&source, message)
                    }
                    OperationType::ConfigUpdate => {
                        self.convert_config_update_metadata(&source, message)
                    }
                    OperationType::FirmwareUpdate => {
                        self.register_firmware_update_operation(&source)
                    }
                    OperationType::Custom(c8y_op_name) => {
                        self.register_custom_operation(&source, c8y_op_name)
                    }
                    _ => Ok(vec![]),
                }
            }

            Channel::Command { operation, cmd_id } if self.command_id.is_generator_of(cmd_id) => {
                self.active_commands.insert(cmd_id.clone());
                let res = match operation {
                    OperationType::Restart => {
                        self.publish_restart_operation_status(&source, cmd_id, message)
                            .await
                    }
                    OperationType::SoftwareList => {
                        self.publish_software_list(&source, cmd_id, message).await
                    }
                    OperationType::SoftwareUpdate => {
                        self.publish_software_update_status(&source, cmd_id, message)
                            .await
                    }
                    OperationType::LogUpload => {
                        self.handle_log_upload_state_change(&source, cmd_id, message)
                            .await
                    }
                    OperationType::ConfigSnapshot => {
                        self.handle_config_snapshot_state_change(&source, cmd_id, message)
                            .await
                    }
                    OperationType::ConfigUpdate => {
                        self.handle_config_update_state_change(&source, cmd_id, message)
                            .await
                    }
                    OperationType::FirmwareUpdate => {
                        self.handle_firmware_update_state_change(&source, cmd_id, message)
                            .await
                    }
                    _ => Ok((vec![], None)),
                };

                match res {
                    // If there are mapped final status messages to be published, they are cached until the operation log is uploaded
                    Ok((messages, Some(command))) if !messages.is_empty() => Ok(self
                        .upload_operation_log(&source, cmd_id, operation, command, messages)
                        .await),
                    Ok((messages, _)) => Ok(messages),
                    Err(e) => Err(e),
                }
            }

            Channel::Health => self.process_health_status_message(&source, message).await,

            _ => Ok(vec![]),
        }
    }

    async fn process_cached_entity_data(
        &mut self,
        cached_entity: PendingEntityData,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut converted_messages = vec![];
        for message in cached_entity.data_messages {
            let (source, channel) = self.mqtt_schema.entity_channel_of(&message.topic).unwrap();
            converted_messages.append(
                &mut self
                    .try_convert_data_message(source, channel, &message)
                    .await?,
            );
        }

        Ok(converted_messages)
    }

    fn validate_operation_supported(
        &self,
        op_type: &OperationType,
        topic_id: &EntityTopicId,
    ) -> Result<(), ConversionError> {
        let target = self.entity_store.try_get(topic_id)?;

        match target.r#type {
            EntityType::MainDevice => Ok(()),
            EntityType::ChildDevice => Ok(()),
            EntityType::Service => Err(ConversionError::UnexpectedError(anyhow!(
                "{op_type} operation for services are currently unsupported"
            ))),
        }
    }

    /// Return the MQTT representation of the entity registration message itself
    /// along with its converted c8y equivalent.
    fn try_convert_auto_registered_entity(
        &mut self,
        registration_message: &EntityRegistrationMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut registration_messages = vec![];
        registration_messages.push(self.convert_entity_registration_message(registration_message));
        let mut c8y_message = self.try_convert_entity_registration(registration_message)?;
        registration_messages.append(&mut c8y_message);

        Ok(registration_messages)
    }

    fn convert_entity_registration_message(
        &self,
        value: &EntityRegistrationMessage,
    ) -> MqttMessage {
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

        register_payload.extend(value.other.clone());

        MqttMessage::new(
            &Topic::new(&format!("{}/{entity_topic_id}", self.mqtt_schema.root)).unwrap(),
            serde_json::to_string(&Value::Object(register_payload)).unwrap(),
        )
        .with_retain()
    }

    async fn try_convert_tedge_and_c8y_topics(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let messages = match &message.topic {
            topic if topic.name.starts_with(INTERNAL_ALARMS_TOPIC) => {
                self.alarm_converter.process_internal_alarm(message);
                Ok(vec![])
            }
            topic if C8yDeviceControlTopic::accept(topic, &self.config.c8y_prefix) => {
                self.parse_c8y_devicecontrol_topic(message).await
            }
            topic if topic.name.starts_with(self.config.c8y_prefix.as_str()) => {
                self.parse_c8y_smartrest_topics(message).await
            }
            _ => {
                error!("Unsupported topic: {}", message.topic.name);
                Ok(vec![])
            }
        }?;

        Ok(messages)
    }

    fn try_init_messages(&mut self) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut messages = self.parse_base_inventory_file()?;

        let supported_operations_message = self.create_supported_operations(
            self.config.ops_dir.as_std_path(),
            &self.config.c8y_prefix,
        )?;

        let device_data_message = self.inventory_device_type_update_message()?;

        let pending_operations_message =
            create_get_pending_operations_message(&self.config.c8y_prefix)?;

        messages.append(&mut vec![
            supported_operations_message,
            device_data_message,
            pending_operations_message,
        ]);
        Ok(messages)
    }

    fn create_supported_operations(
        &self,
        path: &Path,
        prefix: &TopicPrefix,
    ) -> Result<MqttMessage, ConversionError> {
        let topic = if is_child_operation_path(path) {
            let child_id = get_child_external_id(path)?;
            let child_external_id = Self::validate_external_id(&child_id)?;

            C8yTopic::ChildSmartRestResponse(child_external_id.into()).to_topic(prefix)?
        } else {
            C8yTopic::upstream_topic(prefix)
        };

        Ok(MqttMessage::new(
            &topic,
            Operations::try_new(path)?.create_smartrest_ops_message(),
        ))
    }

    pub fn sync_messages(&mut self) -> Vec<MqttMessage> {
        let sync_messages: Vec<MqttMessage> = self.alarm_converter.sync();
        self.alarm_converter = AlarmConverter::Synced;
        sync_messages
    }

    fn try_process_operation_update_message(
        &mut self,
        message: &DiscoverOp,
    ) -> Result<Option<MqttMessage>, ConversionError> {
        let needs_cloud_update = self.update_operations(&message.ops_dir)?;

        if needs_cloud_update {
            Ok(Some(self.create_supported_operations(
                &message.ops_dir,
                &self.config.c8y_prefix,
            )?))
        } else {
            Ok(None)
        }
    }
}

// FIXME: this only extracts the final component of the path, the path prefix can be anything. this
// should be simplified
fn get_child_external_id(dir_path: &Path) -> Result<String, ConversionError> {
    match dir_path.file_name() {
        Some(child_id) => {
            let child_id = child_id.to_string_lossy().to_string();
            Ok(child_id)
        }
        // only returned when path is empty, e.g. "/", in practice this shouldn't ever be given as
        // input
        None => Err(ConversionError::DirPathComponentError {
            dir: dir_path.to_owned(),
        }),
    }
}

fn create_get_pending_operations_message(
    prefix: &TopicPrefix,
) -> Result<MqttMessage, ConversionError> {
    let topic = C8yTopic::SmartRestResponse.to_topic(prefix)?;
    Ok(MqttMessage::new(&topic, request_pending_operations()))
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

impl CumulocityConverter {
    /// Register on C8y an operation capability for a device.
    pub fn register_operation(
        &mut self,
        target: &EntityTopicId,
        c8y_operation_name: &str,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let device = self.entity_store.try_get(target)?;
        let ops_dir = match device.r#type {
            EntityType::MainDevice => self.config.ops_dir.clone(),
            EntityType::ChildDevice => {
                let child_dir_name = device.external_id.as_ref();
                self.config.ops_dir.join(child_dir_name).into()
            }
            EntityType::Service => {
                let target = &device.topic_id;
                error!("Unsupported {c8y_operation_name} operation for a service: {target}");
                return Ok(vec![]);
            }
        };
        let ops_file = ops_dir.join(c8y_operation_name);
        create_directory_with_defaults(&*ops_dir)?;
        create_file_with_defaults(ops_file, None)?;

        let need_cloud_update = self.update_operations(ops_dir.as_std_path())?;

        if need_cloud_update {
            let device_operations =
                self.create_supported_operations(ops_dir.as_std_path(), &self.config.c8y_prefix)?;
            return Ok(vec![device_operations]);
        }

        Ok(vec![])
    }

    /// Saves a new supported operation set for a given device.
    ///
    /// If the supported operation set changed, `Ok(true)` is returned to denote that this change
    /// should be sent to the cloud.
    fn update_operations(&mut self, dir: &Path) -> Result<bool, ConversionError> {
        let operations = get_operations(dir)?;
        let current_operations = if is_child_operation_path(dir) {
            let child_id = get_child_external_id(dir)?;
            let Some(current_operations) = self.children.get_mut(&child_id) else {
                self.children.insert(child_id, operations);
                return Ok(true);
            };
            current_operations
        } else {
            &mut self.operations
        };

        let modified = *current_operations != operations;
        *current_operations = operations;

        Ok(modified)
    }

    async fn register_restart_operation(
        &mut self,
        target: &EntityTopicId,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        match self.register_operation(target, "c8y_Restart") {
            Err(_) => {
                error!("Fail to register `restart` operation for unknown device: {target}");
                Ok(vec![])
            }
            Ok(messages) => Ok(messages),
        }
    }

    async fn publish_restart_operation_status(
        &mut self,
        target: &EntityTopicId,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<(Vec<MqttMessage>, Option<GenericCommandState>), ConversionError> {
        let command = match RestartCommand::try_from(
            target.clone(),
            cmd_id.to_owned(),
            message.payload_bytes(),
        )? {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok((vec![], None));
            }
        };
        let topic = self
            .entity_store
            .get(target)
            .and_then(|entity| C8yTopic::smartrest_response_topic(entity, &self.config.c8y_prefix))
            .ok_or_else(|| Error::UnknownEntity(target.to_string()))?;

        let messages = match command.status() {
            CommandStatus::Executing => {
                let smartrest_set_operation =
                    set_operation_executing(CumulocitySupportedOperations::C8yRestartRequest);
                vec![MqttMessage::new(&topic, smartrest_set_operation)]
            }
            CommandStatus::Successful => {
                let smartrest_set_operation =
                    succeed_operation_no_payload(CumulocitySupportedOperations::C8yRestartRequest);

                vec![
                    command.clearing_message(&self.mqtt_schema),
                    MqttMessage::new(&topic, smartrest_set_operation),
                ]
            }
            CommandStatus::Failed { ref reason } => {
                let smartrest_set_operation = fail_operation(
                    CumulocitySupportedOperations::C8yRestartRequest,
                    &format!("Restart Failed: {reason}"),
                );

                vec![
                    command.clearing_message(&self.mqtt_schema),
                    MqttMessage::new(&topic, smartrest_set_operation),
                ]
            }
            _ => {
                // The other states are ignored
                vec![]
            }
        };

        Ok((
            messages,
            Some(command.into_generic_command(&self.mqtt_schema)),
        ))
    }

    fn register_custom_operation(
        &mut self,
        target: &EntityTopicId,
        c8y_op_name: &str,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        match self.register_operation(target, c8y_op_name) {
            Err(_) => {
                error!("Fail to register `{c8y_op_name}` operation for entity: {target}");
                Ok(vec![])
            }
            Ok(messages) => Ok(messages),
        }
    }

    async fn register_software_list_operation(
        &self,
        target: &EntityTopicId,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if !self.config.software_management_with_types {
            debug!("Publishing c8y_SupportedSoftwareTypes is disabled. To enable it, run `tedge config set c8y.software_management.with_types true`.");
            return Ok(vec![]);
        }

        // Send c8y_SupportedSoftwareTypes, which is introduced in c8y >= 10.14
        let data = SoftwareCommandMetadata::from_json(message.payload_str()?)?;
        let payload = json!({"c8y_SupportedSoftwareTypes": data.types}).to_string();
        let topic = self.get_inventory_update_topic(target)?;

        Ok(vec![MqttMessage::new(&topic, payload)])
    }

    async fn register_software_update_operation(
        &mut self,
        target: &EntityTopicId,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut registration = match self.register_operation(target, "c8y_SoftwareUpdate") {
            Err(_) => {
                error!("Fail to register `software-list` operation for unknown device: {target}");
                return Ok(vec![]);
            }
            Ok(messages) => messages,
        };

        registration.push(self.request_software_list(target));
        Ok(registration)
    }

    async fn publish_software_update_status(
        &mut self,
        target: &EntityTopicId,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<(Vec<MqttMessage>, Option<GenericCommandState>), ConversionError> {
        let command = match SoftwareUpdateCommand::try_from(
            target.clone(),
            cmd_id.to_string(),
            message.payload_bytes(),
        )? {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok((vec![], None));
            }
        };

        let topic = self
            .entity_store
            .get(target)
            .and_then(|entity| C8yTopic::smartrest_response_topic(entity, &self.config.c8y_prefix))
            .ok_or_else(|| Error::UnknownEntity(target.to_string()))?;

        let messages = match command.status() {
            CommandStatus::Init | CommandStatus::Scheduled | CommandStatus::Unknown => {
                // The command has not been processed yet
                vec![]
            }
            CommandStatus::Executing => {
                let smartrest_set_operation_status =
                    set_operation_executing(CumulocitySupportedOperations::C8ySoftwareUpdate);
                vec![MqttMessage::new(&topic, smartrest_set_operation_status)]
            }
            CommandStatus::Successful => {
                let smartrest_set_operation =
                    succeed_operation_no_payload(CumulocitySupportedOperations::C8ySoftwareUpdate);

                vec![
                    MqttMessage::new(&topic, smartrest_set_operation),
                    command.clearing_message(&self.mqtt_schema),
                    self.request_software_list(target),
                ]
            }
            CommandStatus::Failed { reason } => {
                let smartrest_set_operation =
                    fail_operation(CumulocitySupportedOperations::C8ySoftwareUpdate, &reason);

                vec![
                    MqttMessage::new(&topic, smartrest_set_operation),
                    command.clearing_message(&self.mqtt_schema),
                    self.request_software_list(target),
                ]
            }
        };

        Ok((
            messages,
            Some(command.into_generic_command(&self.mqtt_schema)),
        ))
    }

    pub async fn upload_operation_log(
        &mut self,
        target: &EntityTopicId,
        cmd_id: &str,
        op_type: &OperationType,
        command: GenericCommandState,
        final_messages: Vec<MqttMessage>,
    ) -> Vec<MqttMessage> {
        if command.is_finished()
            && command.get_log_path().is_some()
            && (self.config.auto_log_upload == AutoLogUpload::Always
                || (self.config.auto_log_upload == AutoLogUpload::OnFailure && command.is_failed()))
        {
            let log_path = command.get_log_path().unwrap();
            let event_type = format!("{}_op_log", op_type);
            let event_text = format!("{} operation log", &op_type);
            match self
                .upload_file(
                    target,
                    &log_path,
                    None,
                    None,
                    cmd_id,
                    event_type,
                    Some(event_text),
                )
                .await
            {
                Ok(_) => {
                    self.pending_upload_operations
                        .insert(cmd_id.into(), UploadOperationLog { final_messages }.into());
                    return vec![];
                }
                Err(err) => {
                    error!("Operation log upload failed due to {}", err);
                }
            }
        }
        final_messages
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn upload_file(
        &mut self,
        topic_id: &EntityTopicId,
        file_path: &Utf8Path,
        file_name: Option<String>,
        mime_type: Option<Mime>,
        cmd_id: &str,
        event_type: String,
        event_text: Option<String>,
    ) -> Result<Url, ConversionError> {
        let target = self.entity_store.try_get(topic_id)?;
        let xid = target.external_id.as_ref();

        let create_event = CreateEvent {
            event_type: event_type.clone(),
            time: OffsetDateTime::now_utc(),
            text: event_text.unwrap_or(event_type),
            extras: HashMap::new(),
            device_id: xid.into(),
        };
        let event_response_id = self.http_proxy.send_event(create_event).await?;
        let binary_upload_event_url = self
            .c8y_endpoint
            .get_url_for_event_binary_upload_unchecked(&event_response_id);

        let proxy_url = self.auth_proxy.proxy_url(binary_upload_event_url.clone());

        let file_name = file_name.unwrap_or_else(|| {
            format!(
                "{xid}_{filename}",
                filename = file_path.file_name().unwrap_or(cmd_id)
            )
        });
        let form_data = if let Some(mime) = mime_type {
            FormData::new(file_name).set_mime(mime)
        } else {
            FormData::new(file_name)
        };
        // The method must be POST, otherwise file name won't be supported.
        let upload_request = UploadRequest::new(proxy_url.as_str(), file_path)
            .post()
            .with_content_type(ContentType::FormData(form_data));

        self.uploader_sender
            .send((cmd_id.into(), upload_request))
            .await?;

        Ok(binary_upload_event_url)
    }

    async fn publish_software_list(
        &mut self,
        target: &EntityTopicId,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<(Vec<MqttMessage>, Option<GenericCommandState>), ConversionError> {
        let command = match SoftwareListCommand::try_from(
            target.clone(),
            cmd_id.to_owned(),
            message.payload_bytes(),
        )? {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok((Vec::new(), None));
            }
        };

        let messages = match command.status() {
            CommandStatus::Successful => {
                // Send a list via HTTP to support backwards compatibility to c8y < 10.14
                if self.config.software_management_api == SoftwareManagementApiFlag::Legacy {
                    if let Some(device) = self.entity_store.get(target) {
                        let c8y_software_list: C8yUpdateSoftwareListResponse = (&command).into();
                        self.http_proxy
                            .send_software_list_http(
                                c8y_software_list,
                                device.external_id.as_ref().to_string(),
                            )
                            .await?;
                    }
                    return Ok((vec![command.clearing_message(&self.mqtt_schema)], None));
                }

                // Send a list via SmartREST, "advanced software list" feature c8y >= 10.14
                let topic = self
                    .entity_store
                    .get(target)
                    .and_then(|entity| {
                        C8yTopic::smartrest_response_topic(entity, &self.config.c8y_prefix)
                    })
                    .ok_or_else(|| Error::UnknownEntity(target.to_string()))?;
                let payloads =
                    get_advanced_software_list_payloads(&command, SOFTWARE_LIST_CHUNK_SIZE);

                let mut messages: Vec<MqttMessage> = Vec::new();
                for payload in payloads {
                    messages.push(MqttMessage::new(&topic, payload))
                }
                messages.push(command.clearing_message(&self.mqtt_schema));
                messages
            }

            CommandStatus::Failed { reason } => {
                error!("Fail to list installed software packages: {reason}");
                vec![command.clearing_message(&self.mqtt_schema)]
            }

            CommandStatus::Init
            | CommandStatus::Scheduled
            | CommandStatus::Executing
            | CommandStatus::Unknown => {
                // C8Y doesn't expect any message to be published
                Vec::new()
            }
        };

        Ok((
            messages,
            Some(command.into_generic_command(&self.mqtt_schema)),
        ))
    }
}

/// Lists all the locally available child devices linked to this parent device.
///
/// The set of all locally available child devices is defined as any directory
/// created under "`config_dir`/operations/c8y" for example "/etc/tedge/operations/c8y"
pub fn get_local_child_devices_list(path: &Path) -> Result<HashSet<String>, CumulocityMapperError> {
    Ok(fs::read_dir(path)
        .map_err(|_| CumulocityMapperError::ReadDirError {
            dir: PathBuf::from(&path),
        })?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<PathBuf>, _>>()?
        .into_iter()
        .filter(|path| path.is_dir())
        .map(|entry| entry.file_name().unwrap().to_string_lossy().to_string()) // safe unwrap
        .collect::<HashSet<String>>())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::CumulocityConverter;
    use crate::actor::IdDownloadRequest;
    use crate::actor::IdDownloadResult;
    use crate::actor::IdUploadRequest;
    use crate::actor::IdUploadResult;
    use crate::config::C8yMapperConfig;
    use crate::Capabilities;
    use anyhow::Result;
    use assert_json_diff::assert_json_eq;
    use assert_json_diff::assert_json_include;
    use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
    use c8y_api::smartrest::operations::ResultFormat;
    use c8y_api::smartrest::topic::C8yTopic;
    use c8y_auth_proxy::url::Protocol;
    use c8y_auth_proxy::url::ProxyUrlGenerator;
    use c8y_http_proxy::handle::C8YHttpProxy;
    use c8y_http_proxy::messages::C8YRestRequest;
    use c8y_http_proxy::messages::C8YRestResult;
    use serde_json::json;
    use serde_json::Value;
    use std::str::FromStr;
    use tedge_actors::test_helpers::FakeServerBox;
    use tedge_actors::test_helpers::FakeServerBoxBuilder;
    use tedge_actors::Builder;
    use tedge_actors::CloneSender;
    use tedge_actors::LoggingSender;
    use tedge_actors::MessageReceiver;
    use tedge_actors::Sender;
    use tedge_actors::SimpleMessageBoxBuilder;
    use tedge_api::entity_store::EntityRegistrationMessage;
    use tedge_api::entity_store::EntityType;
    use tedge_api::entity_store::InvalidExternalIdError;
    use tedge_api::mqtt_topics::ChannelFilter;
    use tedge_api::mqtt_topics::EntityFilter;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_api::mqtt_topics::OperationType;
    use tedge_api::SoftwareUpdateCommand;
    use tedge_config::AutoLogUpload;
    use tedge_config::SoftwareManagementApiFlag;
    use tedge_config::TEdgeConfig;
    use tedge_mqtt_ext::test_helpers::assert_messages_matching;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;

    #[tokio::test]
    async fn test_sync_alarms() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let alarm_topic = "te/device/main///a/temperature_alarm";
        let alarm_payload = r#"{ "severity": "critical", "text": "Temperature very high" }"#;
        let alarm_message = MqttMessage::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

        // During the sync phase, alarms are not converted immediately, but only cached to be synced later
        assert!(converter.convert(&alarm_message).await.is_empty());

        let non_alarm_topic = "te/device/main///m/";
        let non_alarm_payload = r#"{"temp": 1}"#;
        let non_alarm_message =
            MqttMessage::new(&Topic::new_unchecked(non_alarm_topic), non_alarm_payload);

        // But non-alarms are converted immediately, even during the sync phase
        assert!(!converter.convert(&non_alarm_message).await.is_empty());

        let internal_alarm_topic = "c8y-internal/alarms/te/device/main///a/pressure_alarm";
        let internal_alarm_payload = r#"{ "severity": "major", "text": "Temperature very high" }"#;
        let internal_alarm_message = MqttMessage::new(
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
        let alarm_message = MqttMessage::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

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

        let second_msg = MqttMessage::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:external_sensor,external_sensor,thin-edge.io-child",
        );
        assert_eq!(device_creation_msgs[1], second_msg);

        // During the sync phase, alarms are not converted immediately, but only cached to be synced later
        assert!(converter.convert(&alarm_message).await.is_empty());

        let non_alarm_topic = "te/device/external_sensor///m/";
        let non_alarm_payload = r#"{"temp": 1}"#;
        let non_alarm_message =
            MqttMessage::new(&Topic::new_unchecked(non_alarm_topic), non_alarm_payload);

        // But non-alarms are converted immediately, even during the sync phase
        assert!(!converter.convert(&non_alarm_message).await.is_empty());

        let internal_alarm_topic =
            "c8y-internal/alarms/te/device/external_sensor///a/pressure_alarm";
        let internal_alarm_payload = r#"{ "severity": "major", "text": "Temperature very high" }"#;
        let internal_alarm_message = MqttMessage::new(
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

        let in_message = MqttMessage::new(
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
        let reg_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/immediate_child//"),
            json!({
                "@type":"child-device",
                "@parent":"device/main//",
                "@id":"immediate_child"
            })
            .to_string(),
        );
        let _ = converter.convert(&reg_message).await;

        let reg_message = MqttMessage::new(
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
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_c8y_json_message = MqttMessage::new(
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
        let reg_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/immediate_child//"),
            json!({
                "@type":"child-device",
                "@parent":"device/main//",
                "@id":"immediate_child"
            })
            .to_string(),
        );
        let _ = converter.convert(&reg_message).await;

        let reg_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/nested_child//"),
            json!({
                "@type":"child-device",
                "@parent":"device/immediate_child//",
                "@id":"nested_child"
            })
            .to_string(),
        );
        let _ = converter.convert(&reg_message).await;

        let reg_message = MqttMessage::new(
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
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_c8y_json_message = MqttMessage::new(
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
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_child_create_msg = MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            json!({
                "@id":"test-device:device:child1",
                "@type":"child-device",
                "name":"child1",
            })
            .to_string(),
        )
        .with_retain();

        let expected_smart_rest_message_child = MqttMessage::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:child1,child1,thin-edge.io-child",
        );
        let expected_service_create_msg = MqttMessage::new(
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

        let expected_smart_rest_message_service = MqttMessage::new(
            &Topic::new_unchecked("c8y/s/us/test-device:device:child1"),
            "102,test-device:device:child1:service:app1,service,app1,up",
        );
        let expected_c8y_json_message = MqttMessage::new(
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
                expected_c8y_json_message.clone(),
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
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_create_service_msg = MqttMessage::new(
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

        let expected_c8y_json_message = MqttMessage::new(
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

        let expected_smart_rest_message_service = MqttMessage::new(
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
                expected_c8y_json_message.clone(),
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
        let in_first_message =
            MqttMessage::new(&Topic::new_unchecked(in_topic), in_invalid_payload);
        let in_second_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_valid_payload);

        // First convert invalid Thin Edge JSON message.
        let out_first_messages = converter.convert(&in_first_message).await;
        let expected_error_message = MqttMessage::new(
            &Topic::new_unchecked("te/errors"),
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
        let expected_smart_rest_message = MqttMessage::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:child1,child1,thin-edge.io-child",
        );
        let expected_c8y_json_message = MqttMessage::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"externalSource":{"externalId":"test-device:device:child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
        );
        assert_eq!(
            out_second_messages,
            vec![expected_smart_rest_message, expected_c8y_json_message]
        );
    }

    #[tokio::test]
    async fn auto_registration_succeeds_even_on_bad_input() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let topic = Topic::new_unchecked("te/device/child1///m/");
        // First convert invalid Thin Edge JSON message.
        let invalid_measurement = MqttMessage::new(&topic, "invalid measurement");
        let messages = converter.convert(&invalid_measurement).await;
        assert_messages_matching(
            &messages,
            [
                (
                    "te/device/child1//",
                    json!({
                        "@id":"test-device:device:child1",
                        "@type":"child-device",
                        "name":"child1",
                    })
                    .into(),
                ),
                (
                    "c8y/s/us",
                    "101,test-device:device:child1,child1,thin-edge.io-child".into(),
                ),
                (
                    "te/errors",
                    "Invalid JSON: expected value at line 1 column 1: `invalid measurement\n`"
                        .into(),
                ),
            ],
        );

        // Second convert valid Thin Edge JSON message.
        let valid_measurement = MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///m/"),
            json!({
                "temp": 50.0,
                "time": "2021-11-16T17:45:40.571760714+01:00"
            })
            .to_string(),
        );

        let messages = converter.convert(&valid_measurement).await;
        assert_messages_matching(
            &messages,
            [(
                "c8y/measurement/measurements/create",
                json!({
                "externalSource": {
                    "externalId":"test-device:device:child1",
                    "type":"c8y_Serial"},
                    "temp":{
                        "temp":{
                            "value": 50.0
                        }
                    },
                    "time":"2021-11-16T17:45:40.571760714+01:00",
                    "type":"ThinEdgeMeasurement"
                })
                .into(),
            )],
        );
    }

    #[tokio::test]
    async fn convert_two_measurement_messages_given_different_child_id() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;

        // First message from "child1"
        let in_first_message =
            MqttMessage::new(&Topic::new_unchecked("te/device/child1///m/"), in_payload);
        let out_first_messages: Vec<_> = converter
            .convert(&in_first_message)
            .await
            .into_iter()
            .filter(|m| m.topic.name.starts_with("c8y"))
            .collect();
        let expected_first_smart_rest_message = MqttMessage::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:child1,child1,thin-edge.io-child",
        );
        let expected_first_c8y_json_message = MqttMessage::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"externalSource":{"externalId":"test-device:device:child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
        );
        assert_eq!(
            out_first_messages,
            vec![
                expected_first_smart_rest_message,
                expected_first_c8y_json_message,
            ]
        );

        // Second message from "child2"
        let in_second_message =
            MqttMessage::new(&Topic::new_unchecked("te/device/child2///m/"), in_payload);
        let out_second_messages: Vec<_> = converter
            .convert(&in_second_message)
            .await
            .into_iter()
            .filter(|m| m.topic.name.starts_with("c8y"))
            .collect();
        let expected_second_smart_rest_message = MqttMessage::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:child2,child2,thin-edge.io-child",
        );
        let expected_second_c8y_json_message = MqttMessage::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"externalSource":{"externalId":"test-device:device:child2","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
        );
        assert_eq!(
            out_second_messages,
            vec![
                expected_second_smart_rest_message,
                expected_second_c8y_json_message,
            ]
        );
    }

    #[tokio::test]
    async fn convert_measurement_with_main_id_with_measurement_type() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "te/device/main///m/test_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_c8y_json_message = MqttMessage::new(
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
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_c8y_json_message = MqttMessage::new(
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
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        let expected_smart_rest_message = MqttMessage::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:child,child,thin-edge.io-child",
        );

        let expected_c8y_json_message = MqttMessage::new(
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
                expected_c8y_json_message.clone(),
            ]
        );
    }

    #[tokio::test]
    async fn convert_measurement_with_child_id_with_measurement_type_in_payload() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "te/device/child2///m/test_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00","type":"type_in_payload"}"#;
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);
        let expected_smart_rest_message = MqttMessage::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:child2,child2,thin-edge.io-child",
        );

        let expected_c8y_json_message = MqttMessage::new(
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
                expected_c8y_json_message.clone(),
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
        let alarm_message = MqttMessage::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

        let messages = converter.try_convert(&alarm_message).await.unwrap();
        let payload = messages[0].payload_str().unwrap();
        assert!(payload.ends_with("greater than the threshold size of 16184."));
        Ok(())
    }

    #[tokio::test]
    async fn convert_event_without_given_event_type() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let event_topic = "te/device/main///e/";
        let event_payload = r#"{ "text": "Someone clicked", "time": "2020-02-02T01:02:03+05:30" }"#;
        let event_message = MqttMessage::new(&Topic::new_unchecked(event_topic), event_payload);

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
        let event_message = MqttMessage::new(&Topic::new_unchecked(event_topic), event_payload);

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
        let event_message = MqttMessage::new(&Topic::new_unchecked(event_topic), event_payload);

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
        let event_message = MqttMessage::new(&Topic::new_unchecked(event_topic), event_payload);

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
    async fn convert_event_with_custom_c8y_topic_prefix() {
        let tmp_dir = TempTedgeDir::new();
        let mut config = c8y_converter_config(&tmp_dir);
        let tedge_config = TEdgeConfig::load_toml_str("service.ty = \"\"");
        config.service = tedge_config.service.clone();
        config.c8y_prefix = "custom-topic".try_into().unwrap();

        let (mut converter, _) = create_c8y_converter_from_config(config);
        let event_topic = "te/device/main///e/click_event";
        let event_payload = r#"{ "text": "Someone clicked", "time": "2020-02-02T01:02:03+05:30" }"#;
        let event_message = MqttMessage::new(&Topic::new_unchecked(event_topic), event_payload);

        let converted_events = converter.convert(&event_message).await;
        assert_eq!(converted_events.len(), 1);
        let converted_event = converted_events.get(0).unwrap();
        assert_eq!(converted_event.topic.name, "custom-topic/s/us");

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
        let event_message = MqttMessage::new(&Topic::new_unchecked(event_topic), event_payload);

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
            if let Some(C8YRestRequest::CreateEvent(_)) = http_proxy.recv().await {
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
        let big_event_message =
            MqttMessage::new(&Topic::new_unchecked(event_topic), big_event_payload);

        assert!(converter.convert(&big_event_message).await.is_empty());
    }

    #[tokio::test]
    async fn test_convert_big_event_for_child_device() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, mut http_proxy) = create_c8y_converter(&tmp_dir).await;
        tokio::spawn(async move {
            if let Some(C8YRestRequest::CreateEvent(_)) = http_proxy.recv().await {
                http_proxy
                    .send(Ok(c8y_http_proxy::messages::C8YRestResponse::EventId(
                        "event-id".into(),
                    )))
                    .await
                    .unwrap()
            }
        });

        let event_topic = "te/device/child1///e/click_event";
        let big_event_text = create_packet((16 + 1) * 1024); // Event payload > size_threshold
        let big_event_payload = json!({ "text": big_event_text }).to_string();
        let big_event_message =
            MqttMessage::new(&Topic::new_unchecked(event_topic), big_event_payload);

        let child_registration_messages = converter.convert(&big_event_message).await;

        for message in child_registration_messages {
            // Event creation message should be handled via HTTP
            assert_ne!(message.topic.name, "c8y/event/events/create")
        }
    }

    #[tokio::test]
    async fn test_convert_big_measurement() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;
        let measurement_topic = "te/device/main///m/";
        let big_measurement_payload = create_thin_edge_measurement(10 * 1024); // Measurement payload > size_threshold after converting to c8y json

        let big_measurement_message = MqttMessage::new(
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

        let big_measurement_message = MqttMessage::new(
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

        let big_measurement_message = MqttMessage::new(
            &Topic::new_unchecked(measurement_topic),
            big_measurement_payload,
        );

        let result = converter.convert(&big_measurement_message).await;

        // Skipping the first two auto-registration messages and validating the third mapped message
        let payload = result[2].payload_str().unwrap();
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

        let big_measurement_message = MqttMessage::new(
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
        assert!(payload2.contains(
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
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        let mqtt_schema = MqttSchema::new();
        let (in_entity, _in_channel) = mqtt_schema.entity_channel_of(&in_message.topic).unwrap();

        let expected_child_create_smart_rest_message = MqttMessage::new(
            &Topic::new_unchecked("c8y/s/us"),
            "101,test-device:device:child1,child1,thin-edge.io-child",
        );

        let expected_service_monitor_smart_rest_message = MqttMessage::new(
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
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        let mqtt_schema = MqttSchema::new();
        let (in_entity, _in_channel) = mqtt_schema.entity_channel_of(&in_message.topic).unwrap();

        let expected_service_monitor_smart_rest_message = MqttMessage::new(
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
                ResultFormat::Text,
                tokio::time::Duration::from_secs(10),
                tokio::time::Duration::from_secs(1),
                "sleep_ten".to_owned(),
            )
            .await
            .unwrap();
        converter
            .execute_operation(
                "5",
                "sleep",
                ResultFormat::Text,
                tokio::time::Duration::from_secs(20),
                tokio::time::Duration::from_secs(1),
                "sleep_twenty".to_owned(),
            )
            .await
            .unwrap();

        // a result between now and elapsed that is not 0 probably means that the operations are
        // blocking and that you probably removed a tokio::spawn handle (;
        assert_eq!(now.elapsed().as_secs(), 0);
    }

    #[tokio::test]
    async fn handle_operations_for_child_device() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        // The child has first to declare its capabilities
        let mqtt_schema = MqttSchema::default();
        let child = EntityTopicId::default_child_device("childId").unwrap();
        let child_capability = SoftwareUpdateCommand::capability_message(&mqtt_schema, &child);
        let registrations = converter.try_convert(&child_capability).await.unwrap();

        // the first message should be auto-registration of chidlId
        let registration = registrations.get(0).unwrap().clone();
        assert_eq!(
            registration,
            MqttMessage::new(
                &Topic::new_unchecked("te/device/childId//"),
                r#"{"@id":"test-device:device:childId","@type":"child-device","name":"childId"}"#,
            )
            .with_retain()
        );

        // the auto-registration message is produced & processed by the mapper
        converter.try_convert(&registration).await.unwrap();

        // A request to a child is forwarded to that child using its registered mapping: external id <=> topic identifier
        let device_cmd_channel = mqtt_schema.topics(
            EntityFilter::Entity(&child),
            ChannelFilter::Command(OperationType::SoftwareUpdate),
        );
        let mqtt_message = MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "c8y_SoftwareUpdate": [
                    {
                        "name": "software_a",
                        "action": "install",
                        "version": "version_a",
                        "url": "url_a"
                    }
                ],
                "externalSource": {
                    "externalId": "test-device:device:childId",
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        );
        let command = converter
            .parse_c8y_devicecontrol_topic(&mqtt_message)
            .await
            .unwrap()
            .get(0)
            .unwrap()
            .clone();

        assert!(device_cmd_channel.accept(&command));
        assert_eq!(
            serde_json::from_slice::<Value>(command.payload_bytes()).unwrap(),
            json!({
                "status":"init",
                "updateList":[
                    { "type":"default",
                      "modules":[
                        {"name":"software_a","version":"version_a","url":"url_a","action":"install"}
                      ]}
                ]
            })
        );
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
                invalid_char,
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

        let measurement_message = MqttMessage::new(
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

    #[tokio::test]
    async fn handles_empty_service_type_2383() {
        let tmp_dir = TempTedgeDir::new();
        let mut config = c8y_converter_config(&tmp_dir);
        let tedge_config = TEdgeConfig::load_toml_str("service.ty = \"\"");
        config.service = tedge_config.service.clone();

        let (mut converter, _) = create_c8y_converter_from_config(config);

        let service_health_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/service1/status/health"),
            serde_json::to_string(&json!({"status": "up"})).unwrap(),
        );

        let output = converter.convert(&service_health_message).await;
        let service_creation_message = output
            .into_iter()
            .find(|m| m.topic == C8yTopic::upstream_topic(&"c8y".try_into().unwrap()))
            .expect("service creation message should be present");

        let mut smartrest_fields = service_creation_message.payload_str().unwrap().split(',');

        assert_eq!(smartrest_fields.next().unwrap(), "102");
        assert_eq!(
            smartrest_fields.next().unwrap(),
            format!("{}:device:main:service:service1", converter.device_name)
        );
        assert_eq!(smartrest_fields.next().unwrap(), "service");
        assert_eq!(smartrest_fields.next().unwrap(), "service1");
        assert_eq!(smartrest_fields.next().unwrap(), "up");
    }

    #[test_case("restart")]
    #[test_case("software_list")]
    #[test_case("software_update")]
    #[test_case("log_upload")]
    #[test_case("config_snapshot")]
    #[test_case("config_update")]
    #[test_case("custom_op")]
    #[tokio::test]
    async fn operations_not_supported_for_services(op_type: &str) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        // Register main device service
        let _ = converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/dummy"),
                json!({
                    "@type":"service",
                })
                .to_string(),
            ))
            .await;
        // Register immediate child device
        let _ = converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/immediate_child//"),
                json!({
                    "@type":"child-device",
                })
                .to_string(),
            ))
            .await;
        // Register immediate child device service
        let _ = converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/immediate_child/service/dummy"),
                json!({
                    "@type":"service",
                })
                .to_string(),
            ))
            .await;
        // Register nested child device
        let _ = converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/nested_child//"),
                json!({
                    "@type":"child-device",
                    "@parent":"device/immediate_child//",
                })
                .to_string(),
            ))
            .await;
        // Register nested child device service
        let _ = converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/nested_child/service/dummy"),
                json!({
                    "@type":"service",
                })
                .to_string(),
            ))
            .await;

        for device_id in ["main", "immediate_child", "nested_child"] {
            let messages = converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked(&format!(
                        "te/device/{device_id}/service/dummy/cmd/{op_type}"
                    )),
                    "[]",
                ))
                .await;
            assert_messages_matching(
                &messages,
                [(
                    "te/errors",
                    "operation for services are currently unsupported".into(),
                )],
            );
        }
    }

    #[tokio::test]
    async fn early_messages_cached_and_processed_only_after_registration() {
        let tmp_dir = TempTedgeDir::new();
        let mut config = c8y_converter_config(&tmp_dir);
        config.enable_auto_register = false;
        config.c8y_prefix = "custom-c8y-prefix".try_into().unwrap();

        let (mut converter, _http_proxy) = create_c8y_converter_from_config(config);

        // Publish some measurements that are only cached and not converted
        for i in 0..3 {
            let measurement_message = MqttMessage::new(
                &Topic::new_unchecked("te/custom/child1///m/environment"),
                json!({ "temperature": i }).to_string(),
            );
            let mapped_messages = converter.convert(&measurement_message).await;
            assert!(
                mapped_messages.is_empty(),
                "Expected the early telemetry messages to be cached and not mapped"
            )
        }

        // Publish a twin message which is also cached
        let twin_message = MqttMessage::new(
            &Topic::new_unchecked("te/custom/child1///twin/foo"),
            r#"5.6789"#,
        );
        let mapped_messages = converter.convert(&twin_message).await;
        assert!(
            mapped_messages.is_empty(),
            "Expected the early twin messages to be cached and not mapped"
        );

        // Publish the registration message which will trigger the conversion of cached messages as well
        let reg_message = MqttMessage::new(
            &Topic::new_unchecked("te/custom/child1//"),
            json!({"@type": "child-device", "@id": "child1", "name": "child1"}).to_string(),
        );
        let messages = converter.convert(&reg_message).await;

        // Assert that the registration message, the twin updates and the cached measurement messages are converted
        assert_messages_matching(
            &messages,
            [
                (
                    "custom-c8y-prefix/s/us",
                    "101,child1,child1,thin-edge.io-child".into(),
                ),
                (
                    "custom-c8y-prefix/inventory/managedObjects/update/child1",
                    json!({
                        "foo": 5.6789
                    })
                    .into(),
                ),
                (
                    "custom-c8y-prefix/measurement/measurements/create",
                    json!({
                        "temperature":{
                            "temperature":{
                                "value": 0.0
                            }
                        },
                    })
                    .into(),
                ),
                (
                    "custom-c8y-prefix/measurement/measurements/create",
                    json!({
                        "temperature":{
                            "temperature":{
                                "value": 1.0
                            }
                        },
                    })
                    .into(),
                ),
                (
                    "custom-c8y-prefix/measurement/measurements/create",
                    json!({
                        "temperature":{
                            "temperature":{
                                "value": 2.0
                            }
                        },
                    })
                    .into(),
                ),
            ],
        );
    }

    #[tokio::test]
    async fn early_child_device_registrations_processed_only_after_parent_registration() {
        let tmp_dir = TempTedgeDir::new();
        let mut config = c8y_converter_config(&tmp_dir);
        config.enable_auto_register = false;

        let (mut converter, _http_proxy) = create_c8y_converter_from_config(config);

        // Publish great-grand-child registration before grand-child and child
        let reg_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/child000//"),
            json!({
                "@type": "child-device",
                "@id": "child000",
                "name": "child000",
                "@parent": "device/child00//",
            })
            .to_string(),
        );
        let messages = converter.convert(&reg_message).await;
        assert!(
            messages.is_empty(),
            "Expected child device registration messages to be cached and not mapped"
        );

        // Publish grand-child registration before child
        let reg_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/child00//"),
            json!({
                "@type": "child-device",
                "@id": "child00",
                "name": "child00",
                "@parent": "device/child0//",
            })
            .to_string(),
        );
        let messages = converter.convert(&reg_message).await;
        assert!(
            messages.is_empty(),
            "Expected child device registration messages to be cached and not mapped"
        );

        // Register the immediate child device which will trigger the conversion of cached messages as well
        let reg_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/child0//"),
            json!({
                "@type": "child-device",
                "@id": "child0",
                "name": "child0",
                "@parent": "device/main//",
            })
            .to_string(),
        );
        let messages = converter.convert(&reg_message).await;

        // Assert that the registration message, the twin updates and the cached measurement messages are converted
        assert_messages_matching(
            &messages,
            [
                ("c8y/s/us", "101,child0,child0,thin-edge.io-child".into()),
                (
                    "c8y/s/us/child0",
                    "101,child00,child00,thin-edge.io-child".into(),
                ),
                (
                    "c8y/s/us/child0/child00",
                    "101,child000,child000,thin-edge.io-child".into(),
                ),
            ],
        );
    }

    pub(crate) async fn create_c8y_converter(
        tmp_dir: &TempTedgeDir,
    ) -> (
        CumulocityConverter,
        FakeServerBox<C8YRestRequest, C8YRestResult>,
    ) {
        let config = c8y_converter_config(tmp_dir);
        create_c8y_converter_from_config(config)
    }

    fn c8y_converter_config(tmp_dir: &TempTedgeDir) -> C8yMapperConfig {
        tmp_dir.dir("operations").dir("c8y");
        tmp_dir.dir("tedge").dir("agent");
        tmp_dir.dir(".tedge-mapper-c8y");

        let device_id = "test-device".into();
        let device_topic_id = EntityTopicId::default_main_device();
        let device_type = "test-device-type".into();
        let tedge_config = TEdgeConfig::load_toml_str("service.ty = \"service\"");
        let c8y_host = "test.c8y.io".into();
        let tedge_http_host = "localhost".into();
        let auth_proxy_addr = "127.0.0.1".into();
        let auth_proxy_port = 8001;
        let auth_proxy_protocol = Protocol::Http;
        let topics = C8yMapperConfig::default_internal_topic_filter(
            &tmp_dir.to_path_buf(),
            &"c8y".try_into().unwrap(),
        )
        .unwrap();

        C8yMapperConfig::new(
            tmp_dir.utf8_path().into(),
            tmp_dir.utf8_path().into(),
            tmp_dir.utf8_path_buf().into(),
            tmp_dir.utf8_path().into(),
            device_id,
            device_topic_id,
            device_type,
            tedge_config.service.clone(),
            c8y_host,
            tedge_http_host,
            topics,
            Capabilities::default(),
            auth_proxy_addr,
            auth_proxy_port,
            auth_proxy_protocol,
            MqttSchema::default(),
            true,
            true,
            "c8y".try_into().unwrap(),
            false,
            SoftwareManagementApiFlag::Advanced,
            true,
            AutoLogUpload::Never,
        )
    }

    fn create_c8y_converter_from_config(
        config: C8yMapperConfig,
    ) -> (
        CumulocityConverter,
        FakeServerBox<C8YRestRequest, C8YRestResult>,
    ) {
        let mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
            SimpleMessageBoxBuilder::new("MQTT", 5);
        let mqtt_publisher = LoggingSender::new("MQTT".into(), mqtt_builder.build().sender_clone());

        let mut c8y_proxy_builder: FakeServerBoxBuilder<C8YRestRequest, C8YRestResult> =
            FakeServerBox::builder();
        let http_proxy = C8YHttpProxy::new(&mut c8y_proxy_builder);

        let auth_proxy_addr = config.auth_proxy_addr.clone();
        let auth_proxy_port = config.auth_proxy_port;
        let auth_proxy = ProxyUrlGenerator::new(auth_proxy_addr, auth_proxy_port, Protocol::Http);

        let uploader_builder: SimpleMessageBoxBuilder<IdUploadResult, IdUploadRequest> =
            SimpleMessageBoxBuilder::new("UL", 5);
        let uploader_sender =
            LoggingSender::new("UL".into(), uploader_builder.build().sender_clone());

        let downloader_builder: SimpleMessageBoxBuilder<IdDownloadResult, IdDownloadRequest> =
            SimpleMessageBoxBuilder::new("DL", 5);
        let downloader_sender =
            LoggingSender::new("DL".into(), downloader_builder.build().sender_clone());

        let converter = CumulocityConverter::new(
            config,
            mqtt_publisher,
            http_proxy,
            auth_proxy,
            uploader_sender,
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
