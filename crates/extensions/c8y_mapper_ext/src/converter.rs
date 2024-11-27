use super::alarm_converter::AlarmConverter;
use super::config::C8yMapperConfig;
use super::error::CumulocityMapperError;
use super::service_monitor;
use crate::actor::CmdId;
use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::dynamic_discovery::DiscoverOp;
use crate::error::ConversionError;
use crate::error::MessageConversionError;
use crate::json;
use crate::operations;
use crate::operations::OperationHandler;
use anyhow::anyhow;
use anyhow::Context;
use c8y_api::json_c8y::C8yCreateEvent;
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
use c8y_api::smartrest::operations::Operation;
use c8y_api::smartrest::operations::Operations;
use c8y_api::smartrest::operations::ResultFormat;
use c8y_api::smartrest::smartrest_serializer::fail_operation_with_id;
use c8y_api::smartrest::smartrest_serializer::fail_operation_with_name;
use c8y_api::smartrest::smartrest_serializer::request_pending_operations;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing_with_id;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing_with_name;
use c8y_api::smartrest::smartrest_serializer::succeed_operation_with_id;
use c8y_api::smartrest::smartrest_serializer::succeed_operation_with_name;
use c8y_api::smartrest::smartrest_serializer::EmbeddedCsv;
use c8y_api::smartrest::smartrest_serializer::TextOrCsv;
use c8y_api::smartrest::topic::publish_topic_from_ancestors;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::CreateEvent;
use plugin_sm::operation_logs::OperationLogs;
use plugin_sm::operation_logs::OperationLogsError;
use serde_json::json;
use serde_json::Value;
use service_monitor::convert_health_status_message;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tedge_actors::ClientMessageBox;
use tedge_actors::LoggingSender;
use tedge_actors::Sender;
use tedge_api::commands::RestartCommand;
use tedge_api::commands::SoftwareCommandMetadata;
use tedge_api::commands::SoftwareListCommand;
use tedge_api::entity_store;
use tedge_api::entity_store::EntityExternalId;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::entity_store::EntityType;
use tedge_api::entity_store::InvalidExternalIdError;
use tedge_api::event::error::ThinEdgeJsonDeserializerError;
use tedge_api::event::ThinEdgeEvent;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::IdGenerator;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::pending_entity_store::PendingEntityData;
use tedge_api::script::ShellScript;
use tedge_api::workflow::GenericCommandState;
use tedge_api::CommandLog;
use tedge_api::DownloadInfo;
use tedge_api::EntityStore;
use tedge_api::Jsonify;
use tedge_api::LoggedCommand;
use tedge_config::TEdgeConfigError;
use tedge_config::TopicPrefix;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_uploader_ext::UploadRequest;
use tedge_uploader_ext::UploadResult;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::create_file_with_defaults;
use tedge_utils::file::create_symlink;
use tedge_utils::file::FileError;
use tedge_utils::size_threshold::SizeThreshold;
use thiserror::Error;
use tokio::time::Duration;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::trace;
use tracing::warn;

const INTERNAL_ALARMS_TOPIC: &str = "c8y-internal/alarms/";
const C8Y_JSON_MQTT_EVENTS_TOPIC: &str = "event/events/create";
const TEDGE_AGENT_LOG_DIR: &str = "agent";
const CREATE_EVENT_SMARTREST_CODE: u16 = 400;
const DEFAULT_EVENT_TYPE: &str = "ThinEdgeEvent";
const FORBIDDEN_ID_CHARS: [char; 3] = ['/', '+', '#'];
const EARLY_MESSAGE_BUFFER_SIZE: usize = 100;

#[derive(Debug)]
pub struct MapperConfig {
    pub out_topic: Topic,
    pub errors_topic: Topic,
}

impl CumulocityConverter {
    pub async fn convert(&mut self, input: &MqttMessage) -> Vec<MqttMessage> {
        let messages_or_err = self.try_convert(input).await;
        self.wrap_errors_with_input(messages_or_err, input)
    }

    pub fn wrap_errors(
        &self,
        messages_or_err: Result<Vec<MqttMessage>, ConversionError>,
    ) -> Vec<MqttMessage> {
        messages_or_err.unwrap_or_else(|error| vec![self.new_error_message(error)])
    }

    pub fn wrap_errors_with_input(
        &self,
        messages_or_err: Result<Vec<MqttMessage>, ConversionError>,
        input: &MqttMessage,
    ) -> Vec<MqttMessage> {
        messages_or_err
            .map_err(|error| MessageConversionError {
                error,
                topic: input.topic.name.clone(),
            })
            .unwrap_or_else(|error| vec![self.new_error_message(error)])
    }

    pub fn new_error_message(&self, error: impl std::error::Error) -> MqttMessage {
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

    pub fn process_operation_update_message(&mut self, message: DiscoverOp) -> Option<MqttMessage> {
        let message_or_err = self.try_process_operation_update_message(&message);
        match message_or_err {
            Ok(Some(msg)) => Some(msg),
            Ok(None) => None,
            Err(err) => Some(self.new_error_message(err)),
        }
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
    pub mqtt_schema: MqttSchema,
    pub entity_store: EntityStore,

    pub command_id: IdGenerator,
    // Keep active command IDs to avoid creation of multiple commands for an operation
    pub active_commands: HashSet<CmdId>,

    pub operation_handler: OperationHandler,
}

impl CumulocityConverter {
    pub fn new(
        config: C8yMapperConfig,
        mqtt_publisher: LoggingSender<MqttMessage>,
        http_proxy: C8YHttpProxy,
        uploader: ClientMessageBox<(String, UploadRequest), (String, UploadResult)>,
        downloader: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
    ) -> Result<Self, CumulocityConverterBuildError> {
        let device_id = config.device_id.clone();
        let device_topic_id = config.device_topic_id.clone();
        let device_type = config.device_type.clone();

        let service_type = if config.service.ty.is_empty() {
            "service".to_owned()
        } else {
            config.service.ty.clone()
        };

        let size_threshold = SizeThreshold(config.max_mqtt_payload_size as usize);

        let prefix = &config.bridge_config.c8y_prefix;

        let operations = Operations::try_new(&*config.ops_dir, &config.bridge_config)?;

        let children = get_child_ops(&*config.ops_dir, &config.bridge_config)?;

        let alarm_converter = AlarmConverter::new();

        let log_dir = config.logs_path.join(TEDGE_AGENT_LOG_DIR);
        let operation_logs = OperationLogs::try_new(log_dir)?;

        let mqtt_schema = config.mqtt_schema.clone();

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

        let command_id = config.id_generator();

        let operation_handler = OperationHandler::new(
            &config,
            downloader,
            uploader,
            mqtt_publisher.clone(),
            http_proxy.clone(),
        );

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
            mqtt_schema: mqtt_schema.clone(),
            entity_store,
            command_id,
            active_commands: HashSet::new(),
            operation_handler,
        })
    }

    /// Try to register the target entity and any of its pending children for the incoming message,
    /// if that target entity is not already registered with the entity store.
    ///
    /// For an entity metadata message (aka registration message),
    /// an attempt is made to register that entity and any previously cached children of that entity.
    /// If the entity can not be registered due to missing parents, it is cached with the entity store to be registered later.
    ///
    /// For any other data messages, auto-registration of the target entities are attempted when enabled.
    ///
    /// In both cases, the successfully registered entities, along with their cached data, is returned.
    pub async fn try_register_source_entities(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<PendingEntityData>, ConversionError> {
        if let Ok((source, channel)) = self.mqtt_schema.entity_channel_of(&message.topic) {
            match channel {
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
                    // if the target entity is unregistered, try to register it first using auto-registration
                    if self.entity_store.get(&source).is_none()
                        && self.config.enable_auto_register
                        && source.matches_default_topic_scheme()
                    {
                        let auto_registered_entities = self.try_auto_register_entity(&source)?;
                        Ok(auto_registered_entities
                            .into_iter()
                            .map(|reg_msg| reg_msg.into())
                            .collect())
                    } else {
                        // On receipt of an unregistered entity data message with custom topic scheme OR
                        // one with default topic scheme itself when auto registration disabled,
                        // since it is received before the entity itself is registered,
                        // cache it in the unregistered entity store to be processed after the entity is registered
                        self.entity_store.cache_early_data_message(message.clone());

                        Ok(vec![])
                    }
                }
            }
        } else {
            Ok(vec![])
        }
    }

    /// Convert an entity registration message based on the context:
    /// that is the kind of message that triggered this registration(channel)
    /// The context is relevant here because of the inconsistency in handling the
    /// auto-registered source entities of a health status message.
    /// For those health messages, the entity registration message is not mapped and ignored
    /// as the status message mapping will create the target entity in the cloud
    /// with the proper initial state derived from the status message itself.
    pub(crate) fn convert_entity_registration_message(
        &mut self,
        message: &EntityRegistrationMessage,
        channel: &Channel,
    ) -> Vec<MqttMessage> {
        let c8y_reg_message = match &channel {
            Channel::EntityMetadata => self.try_convert_entity_registration(message),
            _ => self.try_convert_auto_registered_entity(message, channel),
        };
        self.wrap_errors(c8y_reg_message)
    }

    /// Convert an entity registration message into its C8y counterpart
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
                    &self.config.bridge_config.c8y_prefix,
                )
                .context("Could not create device creation message")?;
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
                    &self.config.bridge_config.c8y_prefix,
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
            &self.config.bridge_config.c8y_prefix,
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
                let smartrest_topic =
                    C8yTopic::upstream_topic(&self.config.bridge_config.c8y_prefix);
                MqttMessage::new(&smartrest_topic, smartrest_event)
            } else {
                // If the message contains extra fields other than `text` and `time`, convert to Cumulocity JSON
                let cumulocity_event_json = serde_json::to_string(&c8y_event)?;
                let json_mqtt_topic = Topic::new_unchecked(&format!(
                    "{}/{C8Y_JSON_MQTT_EVENTS_TOPIC}",
                    self.config.bridge_config.c8y_prefix
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
            &self.config.bridge_config.c8y_prefix,
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
            &self.config.mqtt_schema,
            entity_metadata,
            &ancestors_external_ids,
            message,
            &self.config.bridge_config.c8y_prefix,
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
            .process_json_over_mqtt(
                device_xid,
                operation.op_id.clone(),
                &operation.extras,
                message,
            )
            .await;
        let output = self.handle_c8y_operation_result(&result, Some(operation.op_id));

        Ok(output)
    }

    async fn process_json_over_mqtt(
        &mut self,
        device_xid: String,
        operation_id: String,
        extras: &HashMap<String, Value>,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let cmd_id = self.command_id.new_id_with_str(&operation_id);

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
            C8yDeviceControlOperation::DeviceProfile(request) => {
                if self.config.capabilities.device_profile {
                    if let Some(profile_name) = extras.get("profileName") {
                        self.convert_device_profile_request(
                            device_xid,
                            cmd_id,
                            request,
                            serde_json::from_value(profile_name.clone())?,
                        )?
                    } else {
                        error!("Received a c8y_DeviceProfile without a profile name");
                        vec![]
                    }
                } else {
                    warn!("Received a c8y_DeviceProfile operation, however, device_profile feature is disabled");
                    vec![]
                }
            }
            C8yDeviceControlOperation::Custom => {
                return self
                    .process_json_custom_operation(
                        operation_id,
                        cmd_id,
                        device_xid,
                        extras,
                        message,
                    )
                    .await;
            }
        };

        Ok(msgs)
    }

    async fn parse_json_custom_operation_topic(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let operation = C8yOperation::from_json(message.payload.as_str()?)?;
        let cmd_id = self.command_id.new_id_with_str(&operation.op_id);
        let device_xid = operation.external_source.external_id;

        if self.active_commands.contains(&cmd_id) {
            info!("{cmd_id} is already addressed");
            return Ok(vec![]);
        }

        let result = self
            .process_json_custom_operation(
                operation.op_id.clone(),
                cmd_id,
                device_xid,
                &operation.extras,
                message,
            )
            .await;

        let output = self.handle_c8y_operation_result(&result, Some(operation.op_id));

        Ok(output)
    }

    async fn process_json_custom_operation(
        &self,
        operation_id: String,
        cmd_id: String,
        device_xid: String,
        extras: &HashMap<String, Value>,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let handlers = self.get_operation_handlers(
            device_xid.as_str(),
            &message.topic.name,
            &self.config.bridge_config.c8y_prefix,
        )?;

        if handlers.is_empty() {
            info!("No matched custom operation handler is found for the subscribed custom operation topics. The operation '{operation_id}' (ID) is ignored.");
        }

        for (on_fragment, custom_handler) in &handlers {
            if extras.contains_key(on_fragment) {
                if let Some(command_name) = custom_handler.workflow_operation() {
                    return self.convert_custom_operation_request(
                        device_xid,
                        cmd_id,
                        command_name.to_string(),
                        custom_handler,
                        message,
                    );
                } else {
                    self.execute_custom_operation(custom_handler, message, &operation_id)
                        .await?;
                    break;
                }
            }
        }

        // MQTT messages are sent during the operation execution
        Ok(vec![])
    }

    fn get_operation_handlers(
        &self,
        device_xid: &str,
        topic: &str,
        prefix: &TopicPrefix,
    ) -> Result<Vec<(String, Operation)>, CumulocityMapperError> {
        let entity_xid = device_xid.into();
        let target = self.entity_store.try_get_by_external_id(&entity_xid)?;

        match target.r#type {
            EntityType::MainDevice => Ok(self.operations.filter_by_topic(topic, prefix)),
            EntityType::ChildDevice => {
                if let Some(operations) = self.children.get(device_xid) {
                    Ok(operations.filter_by_topic(topic, prefix))
                } else {
                    Ok(Vec::new())
                }
            }
            EntityType::Service => {
                warn!("operation for services are currently unsupported");
                Ok(Vec::new())
            }
        }
    }

    async fn execute_custom_operation(
        &self,
        custom_handler: &Operation,
        message: &MqttMessage,
        operation_id: &str,
    ) -> Result<(), CumulocityMapperError> {
        let state = GenericCommandState::from_command_message(message).map_err(|e| {
            CumulocityMapperError::JsonCustomOperationHandlerError {
                operation: custom_handler.name.clone(),
                err_msg: format!("Invalid JSON message, {e}. Message: {message:?}"),
            }
        })?;
        let command_value = custom_handler.command().ok_or(
            CumulocityMapperError::JsonCustomOperationHandlerError {
                operation: custom_handler.name.clone(),
                err_msg: "'command' is missing".to_string(),
            },
        )?;
        let script_template = ShellScript::from_str(&command_value).map_err(|e| {
            CumulocityMapperError::JsonCustomOperationHandlerError {
                operation: custom_handler.name.clone(),
                err_msg: format!("Fail to parse the script {command_value}: {e}"),
            }
        })?;
        let script = script_template.inject_values(&state);

        self.execute_operation(
            script,
            custom_handler.result_format(),
            custom_handler.graceful_timeout(),
            custom_handler.forceful_timeout(),
            custom_handler.name.clone(),
            Some(operation_id.into()),
            custom_handler.skip_status_update(),
        )
        .await?;

        Ok(())
    }

    async fn parse_c8y_smartrest_topics(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut output: Vec<MqttMessage> = Vec::new();
        for smartrest_message in collect_smartrest_messages(message.payload_str()?) {
            let result = self.process_smartrest(smartrest_message.as_str()).await;
            let mut msgs = self.handle_c8y_operation_result(&result, None);
            output.append(&mut msgs)
        }
        Ok(output)
    }

    fn handle_c8y_operation_result(
        &mut self,
        result: &Result<Vec<MqttMessage>, CumulocityMapperError>,
        op_id: Option<String>,
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
                    .to_topic(&self.config.bridge_config.c8y_prefix)
                    .unwrap();

                let (payload1, payload2) =
                    if let (Some(op_id), true) = (op_id, self.config.smartrest_use_operation_id) {
                        (
                            set_operation_executing_with_id(&op_id),
                            fail_operation_with_id(&op_id, &err.to_string()),
                        )
                    } else {
                        (
                            set_operation_executing_with_name(operation),
                            fail_operation_with_name(operation, &err.to_string()),
                        )
                    };

                let msg1 = MqttMessage::new(&topic, payload1);
                let msg2 = MqttMessage::new(&topic, payload2);
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
                    if let Ok(package_url) = self.http_proxy.local_proxy_url(url.url()) {
                        *url = DownloadInfo::new(package_url.as_str());
                    }
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
                let script = ShellScript {
                    command,
                    args: vec![payload.to_string()],
                };
                self.execute_operation(
                    script,
                    operation.result_format(),
                    operation.graceful_timeout(),
                    operation.forceful_timeout(),
                    operation.name.clone(),
                    None,
                    operation.skip_status_update(),
                )
                .await?;
            }
        }
        // MQTT messages will be sent during the operation execution
        Ok(vec![])
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_operation(
        &self,
        script: ShellScript,
        result_format: ResultFormat,
        graceful_timeout: Duration,
        forceful_timeout: Duration,
        operation_name: String,
        operation_id: Option<String>,
        skip_status_update: bool,
    ) -> Result<(), CumulocityMapperError> {
        let command = script.command.as_str();
        let cmd_id = self.command_id.new_id();

        let mut logged =
            LoggedCommand::new(command).map_err(|e| CumulocityMapperError::ExecuteFailed {
                error_message: e.to_string(),
                command: command.to_string(),
                operation_name: operation_name.to_string(),
            })?;

        logged.args(script.args);

        let maybe_child_process =
            logged
                .spawn()
                .map_err(|e| CumulocityMapperError::ExecuteFailed {
                    error_message: e.to_string(),
                    command: command.to_string(),
                    operation_name: operation_name.to_string(),
                });

        let log_file = self
            .operation_logs
            .new_log_file(plugin_sm::operation_logs::LogKind::Operation(
                operation_name.to_string(),
            ))
            .await?;
        let mut command_log =
            CommandLog::from_log_path(log_file.path(), operation_name.clone(), cmd_id);

        match maybe_child_process {
            Ok(child_process) => {
                let op_name = operation_name.to_owned();
                let mut mqtt_publisher = self.mqtt_publisher.clone();
                let c8y_prefix = self.config.bridge_config.c8y_prefix.clone();
                let (use_id, op_id) = match operation_id {
                    Some(op_id) if self.config.smartrest_use_operation_id => (true, op_id),
                    _ => (false, "".to_string()),
                };

                tokio::spawn(async move {
                    let op_name = op_name.as_str();
                    let topic = C8yTopic::SmartRestResponse.to_topic(&c8y_prefix).unwrap();

                    if !skip_status_update {
                        // mqtt client publishes executing
                        let executing_str = if use_id {
                            set_operation_executing_with_id(&op_id)
                        } else {
                            set_operation_executing_with_name(op_name)
                        };

                        mqtt_publisher
                            .send(MqttMessage::new(&topic, executing_str.as_str()))
                            .await
                            .unwrap_or_else(|err| {
                                error!("Failed to publish a message: {executing_str}. Error: {err}")
                            });
                    }

                    // execute the command and wait until it finishes
                    // mqtt client publishes failed or successful depending on the exit code
                    if let Ok(output) = child_process
                        .wait_for_output_with_timeout(
                            &mut command_log,
                            graceful_timeout,
                            forceful_timeout,
                        )
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

                                if !skip_status_update {
                                    let success_message = if use_id {
                                        succeed_operation_with_id(&op_id, result)
                                    } else {
                                        succeed_operation_with_name(op_name, result)
                                    };
                                    match success_message {
                                        Ok(message) => mqtt_publisher.send(MqttMessage::new(&topic, message.as_str())).await
                                            .unwrap_or_else(|err| {
                                                error!("Failed to publish a message: {message}. Error: {err}")
                                            }),
                                        Err(e) => {
                                            let reason = format!("{:?}", anyhow::Error::from(e).context("Custom operation process exited successfully, but couldn't convert output to valid SmartREST message"));
                                            let fail_message = if use_id {
                                                fail_operation_with_id(&op_id, &reason)
                                            } else {
                                                fail_operation_with_name(op_name, &reason)
                                            };
                                            mqtt_publisher.send(MqttMessage::new(&topic, fail_message.as_str())).await.unwrap_or_else(|err| {
                                                error!("Failed to publish a message: {fail_message}. Error: {err}")
                                            })
                                        }
                                    }
                                }
                            }
                            _ => {
                                if !skip_status_update {
                                    let failure_reason = get_failure_reason_for_smartrest(
                                        &output.stderr,
                                        MAX_PAYLOAD_LIMIT_IN_BYTES,
                                    );
                                    let payload = if use_id {
                                        fail_operation_with_id(&op_id, &failure_reason)
                                    } else {
                                        fail_operation_with_name(op_name, &failure_reason)
                                    };

                                    mqtt_publisher
                                        .send(MqttMessage::new(&topic, payload.as_str()))
                                        .await
                                        .unwrap_or_else(|err| {
                                            error!(
                                            "Failed to publish a message: {payload}. Error: {err}"
                                        )
                                        })
                                }
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
                // No conversion done here as entity data messages must be converted using pending_entities_from_incoming_message
                Ok(vec![])
            }
            _ => {
                self.try_convert_data_message(source, channel, message)
                    .await
            }
        }
    }

    pub(crate) async fn try_register_entity_with_pending_children(
        &mut self,
        register_message: EntityRegistrationMessage,
    ) -> Result<Vec<PendingEntityData>, ConversionError> {
        match self.entity_store.update(register_message.clone()) {
            Err(e) => {
                error!("Entity registration failed: {e}");
                Ok(vec![])
            }
            Ok((_, pending_entities)) => Ok(pending_entities),
        }
    }

    pub(crate) fn try_auto_register_entity(
        &mut self,
        source: &EntityTopicId,
    ) -> Result<Vec<EntityRegistrationMessage>, ConversionError> {
        if !self.config.enable_auto_register {
            return Err(ConversionError::AutoRegistrationDisabled(
                source.to_string(),
            ));
        }

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

    pub(crate) fn append_id_if_not_given(
        &mut self,
        register_message: &mut EntityRegistrationMessage,
    ) {
        let source = &register_message.topic_id;

        if register_message.external_id.is_none() {
            if let Some(metadata) = self.entity_store.get(source) {
                register_message.external_id = Some(metadata.external_id.clone());
            }
        }
    }

    async fn try_convert_data_message(
        &mut self,
        source: EntityTopicId,
        channel: Channel,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if self.entity_store.get(&source).is_none()
            && !(self.config.enable_auto_register && source.matches_default_topic_scheme())
        {
            // Since the entity is still not present in the entity store,
            // despite an attempt to register the source entity in try_register_source_entities,
            // either auto-registration is disabled or a non-default topic scheme is used.
            // In either case, the message would have been cached in the entity store as pending entity data.
            // Hence just skip the conversion as it will be converted eventually
            // once its source entity is registered.
            return Ok(vec![]);
        }

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
                // https://github.com/thin-edge/thin-edge.io/issues/2739
                if message.payload().is_empty() {
                    warn!(topic = ?message.topic.name, "Ignoring command metadata clearing message: clearing capabilities is not currently supported");
                    return Ok(vec![]);
                }
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
                    OperationType::DeviceProfile => self.register_device_profile_operation(&source),
                    OperationType::Custom(command_name) => {
                        self.register_custom_operation(&source, command_name)
                    }
                    _ => Ok(vec![]),
                }
            }

            Channel::Command { cmd_id, .. } if self.command_id.is_generator_of(cmd_id) => {
                self.active_commands.insert(cmd_id.clone());

                let entity = self.entity_store.try_get(&source)?;
                let external_id = entity.external_id.clone();
                let entity = operations::EntityTarget {
                    topic_id: entity.topic_id.clone(),
                    external_id: external_id.clone(),
                    smartrest_publish_topic: self
                        .smartrest_publish_topic_for_entity(&entity.topic_id)?,
                };

                self.operation_handler.handle(entity, message.clone()).await;
                Ok(vec![])
            }

            Channel::Health => self.process_health_status_message(&source, message).await,

            _ => Ok(vec![]),
        }
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
    pub(crate) fn try_convert_auto_registered_entity(
        &mut self,
        registration_message: &EntityRegistrationMessage,
        channel: &Channel,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut registration_messages = vec![];
        registration_messages.push(
            registration_message
                .clone()
                .to_mqtt_message(&self.mqtt_schema),
        );
        if registration_message.r#type == EntityType::Service && channel.is_health() {
            // If the auto-registration is done on a health status message,
            // no need to map it to a C8y service creation message here,
            // as the status message itself is mapped into a service creation message
            // in try_convert_data_message called after this auto-registration.
            // This avoids redundant service status creation/mapping
            return Ok(registration_messages);
        }

        let mut c8y_message = self.try_convert_entity_registration(registration_message)?;
        registration_messages.append(&mut c8y_message);

        Ok(registration_messages)
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
            topic
                if C8yDeviceControlTopic::accept(topic, &self.config.bridge_config.c8y_prefix) =>
            {
                self.parse_c8y_devicecontrol_topic(message).await
            }
            topic
                if self
                    .operations
                    .get_json_custom_operation_topics()?
                    .accept_topic(topic) =>
            {
                self.parse_json_custom_operation_topic(message).await
            }
            topic
                if self
                    .operations
                    .get_smartrest_custom_operation_topics()?
                    .accept_topic(topic) =>
            {
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

        let supported_operations_message =
            self.create_supported_operations(self.config.ops_dir.as_std_path())?;

        let device_data_message = self.inventory_device_type_update_message()?;

        let pending_operations_message =
            create_get_pending_operations_message(&self.config.bridge_config.c8y_prefix)?;

        messages.append(&mut vec![
            supported_operations_message,
            device_data_message,
            pending_operations_message,
        ]);
        Ok(messages)
    }

    fn create_supported_operations(&self, path: &Path) -> Result<MqttMessage, ConversionError> {
        let is_child = is_child_operation_path(path);
        let device = if is_child {
            let external_id = get_child_external_id(path)?;
            self.entity_store
                .get_by_external_id(&EntityExternalId::from(external_id))
                .expect("should be registered")
        } else {
            self.entity_store
                .get(self.entity_store.main_device())
                .expect("should be registered")
        };

        let operations = self
            .operations_for_device(&device.topic_id)
            .expect("should exist");

        let prefix = &self.config.bridge_config.c8y_prefix;

        let topic = if is_child_operation_path(path) {
            let child_id = get_child_external_id(path)?;
            let child_external_id = Self::validate_external_id(&child_id)?;

            C8yTopic::ChildSmartRestResponse(child_external_id.into()).to_topic(prefix)?
        } else {
            C8yTopic::upstream_topic(prefix)
        };

        Ok(MqttMessage::new(
            &topic,
            operations.create_smartrest_ops_message().into_inner(),
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
            Ok(Some(self.create_supported_operations(&message.ops_dir)?))
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
    ///
    /// Additionally when the target is a child device, operation directory for the device will be loaded and operations
    /// not already registered will be registered.
    ///
    /// Returns a Set Supported Operations (114) message if among registered operations there were new operations that
    /// were not announced to the cloud.
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
                error!(
                    %target,
                    "{c8y_operation_name} operation for services are currently unsupported"
                );
                return Err(ConversionError::UnexpectedError(anyhow!(
                    "{c8y_operation_name} operation for services are currently unsupported"
                )));
            }
        };

        let ops_file = ops_dir.join(c8y_operation_name);

        // Save the operation to the operation directory
        create_directory_with_defaults(&*ops_dir)?;
        if let Some(template_name) = self
            .operations
            .get_template_name_by_operation_name(c8y_operation_name)
        {
            create_symlink(self.config.ops_dir.join(template_name), &ops_file)?;
        } else {
            create_file_with_defaults(&ops_file, None)?;
        };

        let ops_dir = ops_dir.as_std_path();

        let need_cloud_update = match is_child_operation_path(ops_dir) {
            // for devices other than the main device, dynamic update of supported operations via file events is
            // disabled, so we have to additionally load new operations from the c8y operations for that device
            true => self.update_operations(ops_dir)?,

            // for main devices new operation files are loaded dynamically as they are created, so only register one
            // operation we need
            false => {
                let bridge_config = &self.config.bridge_config;
                let operation = c8y_api::smartrest::operations::get_operation(
                    ops_file.as_std_path(),
                    bridge_config,
                )?;

                let current_operations = self
                    .operations_for_device_mut(target)
                    .expect("entity should've been checked before that it's not a service");

                let prev_operation = current_operations.remove_operation(&operation.name);

                // even if the body of the operation is different, as long as it has the same name, supported operations message
                // will be the same, so we don't need to resend
                let need_cloud_update = prev_operation.is_none();

                current_operations.add_operation(operation);

                need_cloud_update
            }
        };

        if need_cloud_update {
            let cloud_update_operations_message = self.create_supported_operations(ops_dir)?;

            return Ok(vec![cloud_update_operations_message]);
        }

        Ok(vec![])
    }

    /// Loads and saves a new supported operation set for a given device.
    ///
    /// All operation files from the given operation directory are loaded and set as the new supported operation set for
    /// a given device. Invalid operation files are ignored.
    ///
    /// If the supported operation set changed, `Ok(true)` is returned to denote that this change should be sent to the
    /// cloud.
    fn update_operations(&mut self, dir: &Path) -> Result<bool, ConversionError> {
        let operations = Operations::try_new(dir, &self.config.bridge_config)?;
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

    fn register_custom_operation(
        &mut self,
        target: &EntityTopicId,
        command_name: &str,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if let Some(c8y_op_name) = self
            .operations
            .get_operation_name_by_workflow_operation(command_name)
        {
            match self.register_operation(target, &c8y_op_name) {
                Err(_) => {
                    error!("Fail to register `{c8y_op_name}` operation for entity: {target}");
                    Ok(vec![])
                }
                Ok(messages) => Ok(messages),
            }
        } else {
            warn!("Failed to find the template file for `{command_name}`. Registration skipped");
            Ok(vec![])
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

    fn operations_for_device(&self, device: &EntityTopicId) -> Option<&Operations> {
        let device = self.entity_store.get(device)?;

        match device.r#type {
            EntityType::MainDevice => Some(&self.operations),
            EntityType::ChildDevice => self.children.get(device.external_id.as_ref()),
            EntityType::Service => None,
        }
    }

    /// Return `Operations` struct for a given entity.
    ///
    /// Returns `None` if an entity is a service, as services can't have operations.
    /// If `Operations` wasn't yet created for a device, this function creates it.
    fn operations_for_device_mut(&mut self, device: &EntityTopicId) -> Option<&mut Operations> {
        let device = self
            .entity_store
            .get(device)
            .expect("Entity should've been registered");

        match device.r#type {
            EntityType::MainDevice => Some(&mut self.operations),
            EntityType::ChildDevice => {
                let key = device.external_id.as_ref();
                // can't avoid the double hash+lookup because of borrow checker limitation
                // https://www.reddit.com/r/rust/comments/qi3ye9/comment/hih04qs/
                if !self.children.contains_key(key) {
                    self.children.insert(key.to_string(), Operations::default());
                }
                self.children.get_mut(key)
            }
            EntityType::Service => None,
        }
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
    use crate::config::BridgeConfig;
    use crate::config::C8yMapperConfig;
    use crate::Capabilities;
    use anyhow::Result;
    use assert_json_diff::assert_json_include;
    use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
    use c8y_api::proxy_url::Protocol;
    use c8y_api::smartrest::operations::ResultFormat;
    use c8y_api::smartrest::topic::C8yTopic;
    use c8y_http_proxy::handle::C8YHttpProxy;
    use serde_json::json;
    use serde_json::Value;
    use std::str::FromStr;
    use tedge_config::TopicPrefix;

    use crate::tests::spawn_dummy_c8y_http_proxy;
    use tedge_actors::test_helpers::FakeServerBox;
    use tedge_actors::test_helpers::FakeServerBoxBuilder;
    use tedge_actors::Builder;
    use tedge_actors::ClientMessageBox;
    use tedge_actors::CloneSender;
    use tedge_actors::LoggingSender;
    use tedge_actors::SimpleMessageBoxBuilder;
    use tedge_api::entity_store::InvalidExternalIdError;
    use tedge_api::mqtt_topics::Channel;
    use tedge_api::mqtt_topics::ChannelFilter;
    use tedge_api::mqtt_topics::EntityFilter;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_api::mqtt_topics::OperationType;
    use tedge_api::pending_entity_store::PendingEntityData;
    use tedge_api::script::ShellScript;
    use tedge_api::SoftwareUpdateCommand;
    use tedge_config::AutoLogUpload;
    use tedge_config::SoftwareManagementApiFlag;
    use tedge_config::TEdgeConfig;
    use tedge_http_ext::HttpRequest;
    use tedge_http_ext::HttpResult;
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

        converter
            .try_register_source_entities(&alarm_message)
            .await
            .unwrap();

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

    #[test_case(
        "m/env",
        json!({ "temp": 1})
        ;"measurement"
    )]
    #[test_case(
        "e/click",
        json!({ "text": "Someone clicked" })
        ;"event"
    )]
    #[test_case(
        "a/temp",
        json!({ "text": "Temperature too high" })
        ;"alarm"
    )]
    #[test_case(
        "twin/custom",
        json!({ "foo": "bar" })
        ;"twin"
    )]
    #[test_case(
        "status/health",
        json!({ "status": "up" })
        ;"health status"
    )]
    #[test_case(
        "cmd/restart",
        json!({ })
        ;"command metadata"
    )]
    #[test_case(
        "cmd/restart/123",
        json!({ "status": "init" })
        ;"command"
    )]
    #[tokio::test]
    async fn auto_registration(channel: &str, payload: Value) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        // Validate auto-registration of child device
        let topic = format!("te/device/child1///{channel}");
        let in_message = MqttMessage::new(&Topic::new_unchecked(&topic), payload.to_string());

        let entities = converter
            .try_register_source_entities(&in_message)
            .await
            .unwrap();
        let messages: Vec<MqttMessage> = entities
            .into_iter()
            .map(|entity| entity.reg_message.to_mqtt_message(&converter.mqtt_schema))
            .collect();

        assert_messages_matching(
            &messages,
            [(
                "te/device/child1//",
                json!({
                    "@type":"child-device",
                    "@id":"test-device:device:child1",
                    "name":"child1"
                })
                .into(),
            )],
        );

        // Validate auto-registration of child device and its service
        let topic = format!("te/device/child2///{channel}");
        let in_message = MqttMessage::new(&Topic::new_unchecked(&topic), payload.to_string());

        let entities = converter
            .try_register_source_entities(&in_message)
            .await
            .unwrap();
        let messages: Vec<MqttMessage> = entities
            .into_iter()
            .map(|entity| entity.reg_message.to_mqtt_message(&converter.mqtt_schema))
            .collect();

        assert_messages_matching(
            &messages,
            [(
                "te/device/child2//",
                json!({
                    "@type":"child-device",
                    "@id":"test-device:device:child2",
                    "name":"child2"
                })
                .into(),
            )],
        );
    }

    #[tokio::test]
    async fn convert_child_device_registration() {
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
        let entities = converter
            .try_register_source_entities(&in_message)
            .await
            .unwrap();

        assert_eq!(entities.len(), 1);
        let messages = converter.convert_entity_registration_message(
            &entities.get(0).unwrap().reg_message,
            &Channel::Measurement {
                measurement_type: "".into(),
            },
        );

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
            ],
        );
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
        converter
            .try_register_source_entities(&in_message)
            .await
            .unwrap();

        let messages = converter.convert(&in_message).await;

        assert_messages_matching(
            &messages,
            [(
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
            )],
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
        let _ = converter
            .try_register_source_entities(&reg_message)
            .await
            .unwrap();

        let reg_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/nested_child//"),
            json!({
                "@type":"child-device",
                "@parent":"device/immediate_child//",
                "@id":"nested_child"
            })
            .to_string(),
        );
        let _ = converter
            .try_register_source_entities(&reg_message)
            .await
            .unwrap();

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
        let _ = converter
            .try_register_source_entities(&reg_message)
            .await
            .unwrap();

        let reg_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/nested_child//"),
            json!({
                "@type":"child-device",
                "@parent":"device/immediate_child//",
                "@id":"nested_child"
            })
            .to_string(),
        );
        let _ = converter
            .try_register_source_entities(&reg_message)
            .await
            .unwrap();

        let reg_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/nested_child/service/nested_service"),
            json!({
                "@type":"service",
                "@parent":"device/nested_child//",
                "@id":"nested_service"
            })
            .to_string(),
        );
        let _ = converter
            .try_register_source_entities(&reg_message)
            .await
            .unwrap();

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

        let _ = converter
            .try_register_source_entities(&in_message)
            .await
            .unwrap();

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

        let out_first_messages = converter.convert(&in_message).await;
        assert_eq!(out_first_messages, vec![expected_c8y_json_message.clone(),]);
    }

    #[tokio::test]
    async fn convert_measurement_for_main_device_service() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "te/device/main/service/appm/m/m_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        let _ = converter
            .try_register_source_entities(&in_message)
            .await
            .unwrap();

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

        let out_first_messages = converter.convert(&in_message).await;
        assert_eq!(out_first_messages, vec![expected_c8y_json_message.clone(),]);
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
        let _ = converter
            .try_register_source_entities(&invalid_measurement)
            .await
            .unwrap();

        let messages = converter.convert(&invalid_measurement).await;
        assert_messages_matching(
            &messages,
            [(
                "te/errors",
                "Invalid JSON: expected value at line 1 column 1: `invalid measurement\n`".into(),
            )],
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

        let _ = converter
            .try_register_source_entities(&in_first_message)
            .await
            .unwrap();

        let out_first_messages: Vec<_> = converter
            .convert(&in_first_message)
            .await
            .into_iter()
            .filter(|m| m.topic.name.starts_with("c8y"))
            .collect();
        let expected_first_c8y_json_message = MqttMessage::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"externalSource":{"externalId":"test-device:device:child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
        );
        assert_eq!(out_first_messages, vec![expected_first_c8y_json_message,]);

        // Second message from "child2"
        let in_second_message =
            MqttMessage::new(&Topic::new_unchecked("te/device/child2///m/"), in_payload);
        let _ = converter
            .try_register_source_entities(&in_second_message)
            .await
            .unwrap();

        let out_second_messages: Vec<_> = converter
            .convert(&in_second_message)
            .await
            .into_iter()
            .filter(|m| m.topic.name.starts_with("c8y"))
            .collect();
        let expected_second_c8y_json_message = MqttMessage::new(
            &Topic::new_unchecked("c8y/measurement/measurements/create"),
            r#"{"externalSource":{"externalId":"test-device:device:child2","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
        );
        assert_eq!(out_second_messages, vec![expected_second_c8y_json_message,]);
    }

    #[tokio::test]
    async fn convert_measurement_with_main_id_with_measurement_type() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "te/device/main///m/test_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        let _ = converter
            .try_register_source_entities(&in_message)
            .await
            .unwrap();

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

        let _ = converter
            .try_register_source_entities(&in_message)
            .await
            .unwrap();

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

        let _ = converter
            .try_register_source_entities(&in_message)
            .await
            .unwrap();

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
        assert_eq!(out_messages, vec![expected_c8y_json_message.clone(),]);
    }

    #[tokio::test]
    async fn convert_measurement_with_child_id_with_measurement_type_in_payload() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let in_topic = "te/device/child2///m/test_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00","type":"type_in_payload"}"#;
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        let _ = converter
            .try_register_source_entities(&in_message)
            .await
            .unwrap();

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
        assert_eq!(out_first_messages, vec![expected_c8y_json_message.clone(),]);
    }

    #[tokio::test]
    async fn check_c8y_threshold_packet_size() -> Result<(), anyhow::Error> {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let alarm_topic = "te/device/main///a/temperature_alarm";
        let big_alarm_text = create_packet(1024 * 20);
        let alarm_payload = json!({ "text": big_alarm_text }).to_string();
        let alarm_message = MqttMessage::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

        let error = converter.try_convert(&alarm_message).await.unwrap_err();
        assert!(matches!(
            error,
            crate::error::ConversionError::SizeThresholdExceeded(_)
        ));

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
        config.bridge_config.c8y_prefix = "custom-topic".try_into().unwrap();

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
        let (mut converter, http_proxy) = create_c8y_converter(&tmp_dir).await;
        spawn_dummy_c8y_http_proxy(http_proxy);

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
        let (mut converter, http_proxy) = create_c8y_converter(&tmp_dir).await;
        spawn_dummy_c8y_http_proxy(http_proxy);

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
        assert!(payload.contains(
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

        converter
            .try_register_source_entities(&big_measurement_message)
            .await
            .unwrap();

        let result = converter.convert(&big_measurement_message).await;

        // Skipping the first two auto-registration messages and validating the third mapped message
        let payload = result[0].payload_str().unwrap();
        assert!(payload.contains(
            r#"The payload {"temperature0":0,"temperature1":1,"temperature10" received on te/device/child1///m/ after translation is"#
        ));
        assert!(payload.ends_with("greater than the threshold size of 16184."));
    }

    #[tokio::test]
    async fn test_execute_operation_is_not_blocked() {
        let tmp_dir = TempTedgeDir::new();
        let (converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let now = std::time::Instant::now();
        converter
            .execute_operation(
                ShellScript::from_str("sleep 5").unwrap(),
                ResultFormat::Text,
                tokio::time::Duration::from_secs(10),
                tokio::time::Duration::from_secs(1),
                "sleep_ten".to_owned(),
                None,
                false,
            )
            .await
            .unwrap();
        converter
            .execute_operation(
                ShellScript::from_str("sleep 5").unwrap(),
                ResultFormat::Text,
                tokio::time::Duration::from_secs(20),
                tokio::time::Duration::from_secs(1),
                "sleep_twenty".to_owned(),
                None,
                false,
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

        converter
            .try_register_source_entities(&child_capability)
            .await
            .unwrap();

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

        let mut entities = converter
            .try_register_source_entities(&measurement_message)
            .await
            .unwrap();
        let local_registration_message = entities.remove(0).reg_message;

        // when converting a registration message the same as the previous one, no additional registration messages should be produced
        let entities = converter
            .try_register_source_entities(
                &local_registration_message.to_mqtt_message(&MqttSchema::default()),
            )
            .await
            .unwrap();

        assert!(entities.is_empty(), "Duplicate entry not registered");
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

        converter
            .try_register_source_entities(&service_health_message)
            .await
            .unwrap();

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
    #[test_case("device_profile")]
    #[test_case("custom_op")]
    #[tokio::test]
    async fn operations_not_supported_for_services(op_type: &str) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        // Register main device service
        let _ = converter
            .try_register_source_entities(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/dummy"),
                json!({
                    "@type":"service",
                })
                .to_string(),
            ))
            .await
            .unwrap();
        // Register immediate child device
        let _ = converter
            .try_register_source_entities(&MqttMessage::new(
                &Topic::new_unchecked("te/device/immediate_child//"),
                json!({
                    "@type":"child-device",
                })
                .to_string(),
            ))
            .await
            .unwrap();
        // Register immediate child device service
        let _ = converter
            .try_register_source_entities(&MqttMessage::new(
                &Topic::new_unchecked("te/device/immediate_child/service/dummy"),
                json!({
                    "@type":"service",
                })
                .to_string(),
            ))
            .await
            .unwrap();
        // Register nested child device
        let _ = converter
            .try_register_source_entities(&MqttMessage::new(
                &Topic::new_unchecked("te/device/nested_child//"),
                json!({
                    "@type":"child-device",
                    "@parent":"device/immediate_child//",
                })
                .to_string(),
            ))
            .await
            .unwrap();
        // Register nested child device service
        let _ = converter
            .try_register_source_entities(&MqttMessage::new(
                &Topic::new_unchecked("te/device/nested_child/service/dummy"),
                json!({
                    "@type":"service",
                })
                .to_string(),
            ))
            .await
            .unwrap();

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
        config.bridge_config.c8y_prefix = "custom-c8y-prefix".try_into().unwrap();

        let (mut converter, _http_proxy) = create_c8y_converter_from_config(config);

        // Publish some measurements that are only cached and not converted
        for i in 0..3 {
            let measurement_message = MqttMessage::new(
                &Topic::new_unchecked("te/custom/child1///m/environment"),
                json!({ "temperature": i }).to_string(),
            );
            converter
                .try_register_source_entities(&measurement_message)
                .await
                .unwrap();
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
        converter
            .try_register_source_entities(&twin_message)
            .await
            .unwrap();
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

        let entities = converter
            .try_register_source_entities(&reg_message)
            .await
            .unwrap();

        let messages = pending_entities_into_mqtt_messages(entities);

        // Assert that the registration message, the twin updates and the cached measurement messages are converted
        assert_messages_matching(
            &messages,
            [
                (
                    "te/custom/child1//",
                    json!({
                        "@id":"child1",
                        "@type":"child-device",
                        "name":"child1"
                    })
                    .into(),
                ),
                ("te/custom/child1///twin/foo", "5.6789".into()),
                (
                    "te/custom/child1///m/environment",
                    json!({ "temperature": 0 }).into(),
                ),
                (
                    "te/custom/child1///m/environment",
                    json!({ "temperature": 1 }).into(),
                ),
                (
                    "te/custom/child1///m/environment",
                    json!({ "temperature": 2 }).into(),
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

        let entities = converter
            .try_register_source_entities(&reg_message)
            .await
            .unwrap();
        assert!(
            entities.is_empty(),
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

        let entities = converter
            .try_register_source_entities(&reg_message)
            .await
            .unwrap();
        assert!(
            entities.is_empty(),
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
        let entities = converter
            .try_register_source_entities(&reg_message)
            .await
            .unwrap();
        let messages = pending_entities_into_mqtt_messages(entities);
        assert_messages_matching(
            &messages,
            [
                (
                    "te/device/child0//",
                    json!({
                        "@type": "child-device",
                        "@id": "child0",
                        "name": "child0",
                        "@parent": "device/main//",
                    })
                    .into(),
                ),
                (
                    "te/device/child00//",
                    json!({
                        "@type": "child-device",
                        "@id": "child00",
                        "name": "child00",
                        "@parent": "device/child0//",
                    })
                    .into(),
                ),
                (
                    "te/device/child000//",
                    json!({
                        "@type": "child-device",
                        "@id": "child000",
                        "name": "child000",
                        "@parent": "device/child00//",
                    })
                    .into(),
                ),
            ],
        );
    }

    /// Check that register_operation correctly registers operations from MQTT and handles error conditions
    #[tokio::test]
    async fn test_register_operation() {
        let tmp_dir = TempTedgeDir::new();
        tmp_dir
            .dir("operations")
            .dir("c8y")
            .file("c8y_Operation.template")
            .with_raw_content(
                r#"[exec]
            on_fragment = "c8y_Operation"
            
            [exec.workflow]
            operation = "my_operation"
            "#,
            );

        let mut config = c8y_converter_config(&tmp_dir);
        config.enable_auto_register = false;

        let (mut converter, _http_proxy) = create_c8y_converter_from_config(config);

        // main device command
        let main_device = converter.entity_store.main_device().clone();

        let operation_topic = converter.mqtt_schema.topic_for(
            &main_device,
            &Channel::CommandMetadata {
                operation: OperationType::Custom("my_operation".to_string()),
            },
        );
        let operation_msg = MqttMessage::new(&operation_topic, "{}");

        let msgs = converter.convert(&operation_msg).await;
        assert_messages_matching(&msgs, [("c8y/s/us", "114,c8y_Operation".into())]);

        // child device command
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

        converter
            .try_register_source_entities(&reg_message)
            .await
            .unwrap();

        let child_device = EntityTopicId::default_child_device("child0").unwrap();
        let operation_topic = converter.mqtt_schema.topic_for(
            &child_device,
            &Channel::CommandMetadata {
                operation: OperationType::Custom("my_operation".to_string()),
            },
        );
        let operation_msg = MqttMessage::new(&operation_topic, "{}");

        let msgs = converter.convert(&operation_msg).await;
        assert_messages_matching(&msgs, [("c8y/s/us/child0", "114,c8y_Operation".into())]);

        // service command
        let reg_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/service0"),
            json!({
                "@type": "service",
                "@id": "service0",
                "name": "service0",
                "@parent": "device/main//",
            })
            .to_string(),
        );

        converter
            .try_register_source_entities(&reg_message)
            .await
            .unwrap();

        let service = main_device.default_service_for_device("service0").unwrap();
        let operation_topic = converter.mqtt_schema.topic_for(
            &service,
            &Channel::CommandMetadata {
                operation: OperationType::Custom("my_operation".to_string()),
            },
        );
        let operation_msg = MqttMessage::new(&operation_topic, "{}");

        let msgs = converter.convert(&operation_msg).await;

        assert_messages_matching(&msgs, [(
                "te/errors",
                "Failed to convert a message on topic 'te/device/main/service/service0/cmd/my_operation': \
                Unexpected error: my_operation operation for services are currently unsupported".into()
            )]);
    }

    fn pending_entities_into_mqtt_messages(entities: Vec<PendingEntityData>) -> Vec<MqttMessage> {
        let mut messages = vec![];
        for entity in entities {
            messages.push(entity.reg_message.to_mqtt_message(&MqttSchema::default()));
            for data_message in entity.data_messages {
                messages.push(data_message);
            }
        }
        messages
    }

    pub(crate) async fn create_c8y_converter(
        tmp_dir: &TempTedgeDir,
    ) -> (CumulocityConverter, FakeServerBox<HttpRequest, HttpResult>) {
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
        let c8y_host = "test.c8y.io".to_owned();
        let tedge_http_host = "127.0.0.1".into();
        let auth_proxy_addr = "127.0.0.1".into();
        let auth_proxy_port = 8001;
        let auth_proxy_protocol = Protocol::Http;
        let bridge_config = BridgeConfig {
            c8y_prefix: TopicPrefix::try_from("c8y").unwrap(),
        };

        let mut topics =
            C8yMapperConfig::default_internal_topic_filter(&"c8y".try_into().unwrap()).unwrap();
        let custom_operation_topics =
            C8yMapperConfig::get_topics_from_custom_operations(tmp_dir.path(), &bridge_config)
                .unwrap();
        topics.add_all(custom_operation_topics);
        topics.remove_overlapping_patterns();

        C8yMapperConfig::new(
            tmp_dir.utf8_path().into(),
            tmp_dir.utf8_path().into(),
            tmp_dir.utf8_path_buf().into(),
            tmp_dir.utf8_path().into(),
            device_id,
            device_topic_id,
            device_type,
            tedge_config.service.clone(),
            c8y_host.clone(),
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
            bridge_config,
            false,
            SoftwareManagementApiFlag::Advanced,
            true,
            AutoLogUpload::Never,
            false,
            16184,
        )
    }

    fn create_c8y_converter_from_config(
        config: C8yMapperConfig,
    ) -> (CumulocityConverter, FakeServerBox<HttpRequest, HttpResult>) {
        let mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
            SimpleMessageBoxBuilder::new("MQTT", 5);
        let mqtt_publisher = LoggingSender::new("MQTT".into(), mqtt_builder.build().sender_clone());

        let mut http_builder: FakeServerBoxBuilder<HttpRequest, HttpResult> =
            FakeServerBox::builder();
        let http_proxy = C8YHttpProxy::new(&config, &mut http_builder);

        let mut uploader_builder: FakeServerBoxBuilder<IdUploadRequest, IdUploadResult> =
            FakeServerBox::builder();
        let uploader = ClientMessageBox::new(&mut uploader_builder);

        let mut downloader_builder: FakeServerBoxBuilder<IdDownloadRequest, IdDownloadResult> =
            FakeServerBox::builder();
        let downloader = ClientMessageBox::new(&mut downloader_builder);

        let converter =
            CumulocityConverter::new(config, mqtt_publisher, http_proxy, uploader, downloader)
                .unwrap();

        (converter, http_builder.build())
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
