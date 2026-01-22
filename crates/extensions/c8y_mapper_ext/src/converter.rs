use super::alarm_converter::AlarmConverter;
use super::config::C8yMapperConfig;
use super::error::CumulocityMapperError;
use super::service_monitor;
use crate::actor::CmdId;
use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::dynamic_discovery::DiscoverOp;
use crate::entity_cache::EntityCache;
use crate::entity_cache::InvalidExternalIdError;
use crate::entity_cache::UpdateOutcome;
use crate::error::ConversionError;
use crate::error::MessageConversionError;
use crate::operations;
use crate::operations::OperationHandler;
use crate::supported_operations::operation::get_child_ops;
use crate::supported_operations::operation::Operation;
use crate::supported_operations::operation::ResultFormat;
use crate::supported_operations::Operations;
use crate::supported_operations::OperationsError;
use crate::supported_operations::SupportedOperations;
use anyhow::Context;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::json_c8y_deserializer::C8yDeviceControlOperation;
use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
use c8y_api::json_c8y_deserializer::C8yJsonOverMqttDeserializerError;
use c8y_api::json_c8y_deserializer::C8yOperation;
use c8y_api::json_c8y_deserializer::C8ySoftwareUpdate;
use c8y_api::smartrest::error::SmartRestDeserializerError;
use c8y_api::smartrest::inventory::child_device_creation_message;
use c8y_api::smartrest::inventory::service_creation_message;
use c8y_api::smartrest::message::collect_smartrest_messages;
use c8y_api::smartrest::message::get_failure_reason_for_smartrest;
use c8y_api::smartrest::message::get_smartrest_device_id;
use c8y_api::smartrest::message::get_smartrest_template_id;
use c8y_api::smartrest::message::sanitize_bytes_for_smartrest;
use c8y_api::smartrest::message::MAX_PAYLOAD_LIMIT_IN_BYTES;
use c8y_api::smartrest::smartrest_serializer::fail_operation_with_id;
use c8y_api::smartrest::smartrest_serializer::fail_operation_with_name;
use c8y_api::smartrest::smartrest_serializer::request_pending_operations;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing_with_id;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing_with_name;
use c8y_api::smartrest::smartrest_serializer::succeed_operation_with_id;
use c8y_api::smartrest::smartrest_serializer::succeed_operation_with_name;
use c8y_api::smartrest::smartrest_serializer::EmbeddedCsv;
use c8y_api::smartrest::smartrest_serializer::TextOrCsv;
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
use tedge_api::entity::EntityExternalId;
use tedge_api::entity::EntityType;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::event::error::ThinEdgeJsonDeserializerError;
use tedge_api::event::ThinEdgeEvent;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::IdGenerator;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::script::ShellScript;
use tedge_api::workflow::GenericCommandState;
use tedge_api::CommandLog;
use tedge_api::DownloadInfo;
use tedge_api::Jsonify;
use tedge_api::LoggedCommand;
use tedge_config::models::TopicPrefix;
use tedge_config::TEdgeConfigError;
use tedge_flows::FlowContextHandle;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_uploader_ext::UploadRequest;
use tedge_uploader_ext::UploadResult;
use tedge_utils::file::FileError;
use tedge_utils::size_threshold::SizeThreshold;
use thiserror::Error;
use tokio::time::Duration;
use tokio::time::Instant;
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
    alarm_converter: AlarmConverter,
    operation_logs: OperationLogs,
    mqtt_publisher: LoggingSender<MqttMessage>,
    pub http_proxy: C8YHttpProxy,
    pub service_type: String,
    pub mqtt_schema: MqttSchema,
    pub(crate) entity_cache: EntityCache,

    pub command_id: IdGenerator,
    // Keep active command IDs to avoid creation of multiple commands for an operation
    pub active_commands: HashMap<CmdId, Option<Instant>>,
    pub recently_completed_commands: HashMap<CmdId, Instant>,
    active_commands_last_cleared: Instant,

    pub supported_operations: SupportedOperations,
    pub operation_handler: OperationHandler,
}

impl CumulocityConverter {
    pub fn new(
        config: C8yMapperConfig,
        mqtt_publisher: LoggingSender<MqttMessage>,
        http_proxy: C8YHttpProxy,
        uploader: ClientMessageBox<(String, UploadRequest), (String, UploadResult)>,
        downloader: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
        flow_context: FlowContextHandle,
    ) -> Result<Self, CumulocityConverterBuildError> {
        let device_id = config.device_id.clone();

        let service_type = if config.service.ty.is_empty() {
            "service".to_owned()
        } else {
            config.service.ty.clone()
        };

        let size_threshold = SizeThreshold(config.max_mqtt_payload_size as usize);

        let operations_by_xid = {
            let mut operations = get_child_ops(&*config.ops_dir, &config.bridge_config)?;
            operations.insert(
                config.device_id.clone(),
                Operations::try_new(&*config.ops_dir, &config.bridge_config)?,
            );
            operations
        };
        let operation_manager = SupportedOperations {
            device_id: device_id.clone(),

            base_ops_dir: Arc::clone(&config.ops_dir),

            operations_by_xid,
        };

        let alarm_converter = AlarmConverter::new();

        let log_dir = config.logs_path.join(TEDGE_AGENT_LOG_DIR);
        let operation_logs = OperationLogs::try_new(log_dir)?;

        let mqtt_schema = config.mqtt_schema.clone();

        let mapper_config = MapperConfig {
            errors_topic: mqtt_schema.error_topic(),
        };

        let entity_cache = EntityCache::new(
            flow_context,
            mqtt_schema.clone(),
            EntityTopicId::default_main_device(),
            device_id.clone().into(),
            Self::map_to_c8y_external_id,
            Self::validate_external_id,
            EARLY_MESSAGE_BUFFER_SIZE,
        );

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
            alarm_converter,
            supported_operations: operation_manager,
            operation_logs,
            http_proxy,
            mqtt_publisher,
            service_type,
            mqtt_schema: mqtt_schema.clone(),
            entity_cache,
            command_id,
            active_commands: HashMap::new(),
            recently_completed_commands: HashMap::new(),
            active_commands_last_cleared: Instant::now(),
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
    pub async fn process_entity_metadata_message(
        &mut self,
        message: &MqttMessage,
    ) -> Result<UpdateOutcome, ConversionError> {
        let (topic_id, channel) = self.mqtt_schema.entity_channel_of(&message.topic).unwrap();
        assert!(channel == Channel::EntityMetadata);
        if message.payload().is_empty() {
            // Clear cached entity
            self.entity_cache.delete(&topic_id);
            return Ok(UpdateOutcome::Deleted);
        }

        let register_message =
            EntityRegistrationMessage::try_from(topic_id, message.payload_bytes())?;
        Ok(self.entity_cache.upsert(register_message.clone())?)
    }

    /// Convert an entity registration message into its C8y counterpart
    pub fn try_convert_entity_registration(
        &mut self,
        source: EntityTopicId,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let input = EntityRegistrationMessage::try_from(source, message.payload_bytes())?;
        let mut messages = vec![];

        // Parse the optional fields
        let display_name = input.twin_data.get("name").and_then(|v| v.as_str());
        let display_type = input.twin_data.get("type").and_then(|v| v.as_str());

        let entity_topic_id = &input.topic_id;
        let external_id = self.entity_cache.try_get_external_id(entity_topic_id)?;
        let reg_message = match input.r#type {
            EntityType::MainDevice => None,
            EntityType::ChildDevice => {
                let parent_xid: Option<&EntityExternalId> =
                    self.entity_cache.parent_external_id(entity_topic_id)?;
                let display_name = display_name.unwrap_or(external_id.as_ref());
                let display_type = display_type.unwrap_or("thin-edge.io-child");

                let child_creation_message = child_device_creation_message(
                    external_id.as_ref(),
                    Some(display_name),
                    Some(display_type),
                    parent_xid.map(|xid| xid.as_ref()),
                    &self.device_name,
                    &self.config.bridge_config.c8y_prefix,
                    self.config.smartrest_child_device_create_with_device_marker,
                )
                .context("Could not create device creation message")?;
                Some(child_creation_message)
            }
            EntityType::Service => {
                let parent_xid = self.entity_cache.parent_external_id(entity_topic_id)?;
                let display_name = display_name.unwrap_or_else(|| {
                    entity_topic_id
                        .default_service_name()
                        .unwrap_or(external_id.as_ref())
                });
                let display_type = display_type.unwrap_or(&self.service_type);

                let service_creation_message = service_creation_message(
                    external_id.as_ref(),
                    display_name,
                    display_type,
                    "up",
                    parent_xid.map(|xid| xid.as_ref()),
                    &self.device_name,
                    &self.config.bridge_config.c8y_prefix,
                )
                .context("Could not create service creation message")?;
                Some(service_creation_message)
            }
        };

        if let Some(reg_message) = reg_message {
            messages.push(reg_message);
        }

        for (fragment_key, fragment_value) in input.twin_data.iter() {
            if fragment_key == "name" || fragment_key == "type" {
                // Skip converting the name and type fields as they are already included in the registration message
                continue;
            }
            let twin_messages = self.convert_twin_fragment(
                entity_topic_id,
                &input.r#type,
                fragment_key,
                fragment_value,
            )?;
            messages.extend(twin_messages);
        }

        Ok(messages)
    }

    /// Return the SmartREST publish topic for the given entity
    /// derived from its ancestors.
    pub fn smartrest_publish_topic_for_entity(
        &self,
        entity_topic_id: &EntityTopicId,
    ) -> Result<Topic, ConversionError> {
        let entity = self.entity_cache.try_get(entity_topic_id)?;
        let topic = C8yTopic::smartrest_response_topic(
            &entity.external_id,
            &entity.metadata.r#type,
            &self.config.bridge_config.c8y_prefix,
        )
        .expect("Topic must have been valid as the external id is pre-validated");
        Ok(topic)
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

        if let Some(entity) = self.entity_cache.get(source) {
            let mqtt_topic = input.topic.name.clone();
            let mqtt_payload = input.payload_str().map_err(|e| {
                ThinEdgeJsonDeserializerError::FailedToParsePayloadToString {
                    topic: mqtt_topic.clone(),
                    error: e.to_string(),
                }
            })?;

            let tedge_event = ThinEdgeEvent::try_from(
                event_type,
                &entity.metadata.r#type,
                &entity.external_id,
                mqtt_payload,
            )
            .map_err(
                |e| ThinEdgeJsonDeserializerError::FailedToParseJsonPayload {
                    topic: mqtt_topic.clone(),
                    error: e.to_string(),
                    payload: mqtt_payload.chars().take(50).collect(),
                },
            )?;

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
        let entity = self.entity_cache.try_get(source)?;

        let mqtt_messages = self.alarm_converter.try_convert_alarm(
            source,
            &entity.external_id,
            &entity.metadata.r#type,
            input,
            alarm_type,
            &self.config.bridge_config.c8y_prefix,
        )?;

        Ok(mqtt_messages)
    }

    pub async fn process_health_status_message(
        &mut self,
        entity_tid: &EntityTopicId,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let entity = self.entity_cache.try_get(entity_tid)?;
        let parent_xid = self.entity_cache.parent_external_id(entity_tid)?;

        Ok(convert_health_status_message(
            &self.config.mqtt_schema,
            entity,
            parent_xid,
            self.entity_cache.main_device_external_id(),
            message,
            &self.config.bridge_config.c8y_prefix,
        ))
    }

    async fn parse_c8y_devicecontrol_topic(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        // JSON over MQTT messages on c8y/devicecontrol/notifications can contain multiple operations in a single MQTT
        // message, so split them
        let operation_payloads = message.payload_str()?.lines();

        let mut output = vec![];
        for operation_payload in operation_payloads {
            let operation = C8yOperation::from_json(operation_payload)?;
            let device_xid = operation.external_source.external_id;
            let cmd_id = self.command_id.new_id_with_str(&operation.op_id);

            if self.command_already_exists(&cmd_id) {
                info!("{cmd_id} is already addressed");
                return Ok(vec![]);
            }

            // wrap operation payload in a dummy MqttMessage wrapper because the code below assumes 1 MQTT message = 1 operation
            // TODO: refactor to avoid this intermediate step and extra copies
            let operation_message = MqttMessage::new(&message.topic, operation_payload);

            let result = self
                .process_json_over_mqtt(
                    device_xid,
                    operation.op_id.clone(),
                    &operation.extras,
                    &operation_message,
                )
                .await;
            let result = self.handle_c8y_operation_result(&result, Some(operation.op_id.clone()));
            self.active_commands.insert(cmd_id, Some(Instant::now()));
            output.extend(result);
        }

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
                if self.config.capabilities.device_restart {
                    self.forward_restart_request(device_xid, cmd_id)?
                } else {
                    warn!("Received a c8y_Restart operation, however, device_restart feature is disabled");
                    vec![]
                }
            }
            C8yDeviceControlOperation::SoftwareUpdate(request) => {
                if self.config.capabilities.software_update {
                    self.forward_software_request(device_xid, cmd_id, request)
                        .await?
                } else {
                    warn!("Received a c8y_SoftwareUpdate operation, however, software_update feature is disabled");
                    vec![]
                }
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

        if self.command_already_exists(&cmd_id) {
            info!("{cmd_id} is already addressed");
            return Ok(vec![]);
        }

        let result = self
            .process_json_custom_operation(
                operation.op_id.clone(),
                cmd_id.clone(),
                device_xid,
                &operation.extras,
                message,
            )
            .await;

        let output = self.handle_c8y_operation_result(&result, Some(operation.op_id));
        self.active_commands.insert(cmd_id, Some(Instant::now()));

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
        let handlers = self.supported_operations.get_operation_handlers(
            &device_xid,
            &message.topic.name,
            &self.config.bridge_config.c8y_prefix,
        );

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
        let target = self.entity_cache.try_get_by_external_id(&entity_xid)?;
        let mut command = software_update_request
            .into_software_update_command(&target.metadata.topic_id, cmd_id)?;

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
        let target = self.entity_cache.try_get_by_external_id(&entity_xid)?;
        let command = RestartCommand::new(&target.metadata.topic_id, cmd_id);
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
        if let Some(operation) = self
            .supported_operations
            .matching_smartrest_template(template)
        {
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
            LoggedCommand::new(command, self.config.tmp_dir.as_ref()).map_err(|e| {
                CumulocityMapperError::ExecuteFailed {
                    error_message: e.to_string(),
                    command: command.to_string(),
                    operation_name: operation_name.to_string(),
                }
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
                    let topic = C8yTopic::upstream_topic(&c8y_prefix);

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

    fn command_already_exists(&self, cmd_id: &str) -> bool {
        self.active_commands.contains_key(cmd_id)
            || self.recently_completed_commands.contains_key(cmd_id)
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
            Channel::EntityMetadata => self.try_convert_entity_registration(source, message),
            _ => {
                self.try_convert_data_message(source, channel, message)
                    .await
            }
        }
    }

    pub(crate) fn append_id_if_not_given(
        &mut self,
        register_message: &mut EntityRegistrationMessage,
    ) {
        let source = &register_message.topic_id;

        if register_message.external_id.is_none() {
            if let Some(metadata) = self.entity_cache.get(source) {
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
        if self.entity_cache.get(&source).is_none() {
            // On receipt of an unregistered entity data message,
            // since it is received before the entity itself is registered,
            // cache it in the unregistered entity store to be processed after the entity is registered
            self.entity_cache.cache_early_data_message(message.clone());
            return Ok(vec![]);
        }

        let entity_type = self.entity_cache.try_get(&source)?.metadata.r#type.clone();
        match &channel {
            Channel::EntityTwinData { fragment_key } => {
                self.try_convert_entity_twin_data(&source, &entity_type, message, fragment_key)
            }

            Channel::Measurement { .. } => Ok(vec![]),

            Channel::Event { event_type } => {
                self.try_convert_event(&source, message, event_type).await
            }

            Channel::Alarm { alarm_type } => {
                self.process_alarm_messages(&source, message, alarm_type)
            }

            Channel::Command { cmd_id, .. } if message.payload_bytes().is_empty() => {
                // The command has been fully processed
                self.active_commands.remove(cmd_id);
                self.recently_completed_commands
                    .insert(cmd_id.to_owned(), Instant::now());
                Ok(vec![])
            }

            Channel::CommandMetadata { operation } => {
                // https://github.com/thin-edge/thin-edge.io/issues/2739
                if message.payload().is_empty() {
                    warn!(topic = ?message.topic.name, "Ignoring command metadata clearing message: clearing capabilities is not currently supported");
                    return Ok(vec![]);
                }
                match operation {
                    OperationType::Restart => self.register_restart_operation(&source).await,
                    OperationType::SoftwareList => {
                        self.register_software_list_operation(&source, message)
                            .await
                    }
                    OperationType::SoftwareUpdate => {
                        self.register_software_update_operation(&source).await
                    }
                    OperationType::LogUpload => self.convert_log_metadata(&source, message).await,
                    OperationType::ConfigSnapshot => {
                        self.convert_config_snapshot_metadata(&source, message)
                            .await
                    }
                    OperationType::ConfigUpdate => {
                        self.convert_config_update_metadata(&source, message).await
                    }
                    OperationType::FirmwareUpdate => {
                        self.register_firmware_update_operation(&source).await
                    }
                    OperationType::DeviceProfile => {
                        self.register_device_profile_operation(&source).await
                    }
                    OperationType::Custom(command_name) => {
                        self.register_custom_operation(&source, command_name).await
                    }
                    _ => Ok(vec![]),
                }
            }

            Channel::Command { cmd_id, .. } if self.command_id.is_generator_of(cmd_id) => {
                // Keep track of operation if we've received it through a retain message
                // If we've already got the operation in `active_commands`, set the insertion
                // time to `None` to disable the time-based expiry
                self.active_commands.insert(cmd_id.clone(), None);

                let entity = self.entity_cache.try_get(&source)?;
                let entity = operations::EntityTarget {
                    topic_id: entity.metadata.topic_id.clone(),
                    external_id: entity.external_id.clone(),
                    smartrest_publish_topic: self
                        .smartrest_publish_topic_for_entity(&entity.metadata.topic_id)?,
                };

                self.operation_handler.handle(entity, message.clone()).await;
                Ok(vec![])
            }

            Channel::Signal { signal_type } => self.process_signal_message(&source, signal_type),

            Channel::Health => self.process_health_status_message(&source, message).await,

            _ => Ok(vec![]),
        }
    }

    async fn try_convert_tedge_and_c8y_topics(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if self.active_commands_last_cleared.elapsed() > Duration::from_secs(3600) {
            let mut to_remove_active = vec![];
            for (id, time) in &self.active_commands {
                if let Some(time) = time {
                    // Expire tasks after 12 hours
                    if time.elapsed() > Duration::from_secs(3600 * 12) {
                        to_remove_active.push(id.to_owned());
                    }
                }
            }
            let mut to_remove_completed = vec![];
            for (id, time) in &self.recently_completed_commands {
                // Remove completed tasks after 1 hour
                if time.elapsed() > Duration::from_secs(3600) {
                    to_remove_completed.push(id.to_owned());
                }
            }
            for id in to_remove_active {
                self.active_commands.remove(&id);
            }
            for id in to_remove_completed {
                self.recently_completed_commands.remove(&id);
            }
            self.active_commands_last_cleared = Instant::now();
        }

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
                    .supported_operations
                    .get_json_custom_operation_topics()?
                    .accept_topic(topic) =>
            {
                self.parse_json_custom_operation_topic(message).await
            }
            topic
                if self
                    .supported_operations
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
        let mut messages = vec![self.c8y_agent_inventory_fragment()?];

        // supported operations for the main device
        let supported_operations_message =
            self.load_and_create_supported_operations_messages(&self.config.device_id.clone())?;
        let pending_operations_message =
            create_get_pending_operations_message(&self.config.bridge_config.c8y_prefix);

        messages.append(&mut vec![
            supported_operations_message,
            pending_operations_message,
        ]);
        Ok(messages)
    }

    pub fn load_and_create_supported_operations_messages(
        &mut self,
        external_id: &str,
    ) -> Result<MqttMessage, ConversionError> {
        self.supported_operations
            .load_all(external_id, &self.config.bridge_config)?;
        let supported_operations_message = self
            .supported_operations
            .create_supported_operations(external_id, &self.config.bridge_config.c8y_prefix)?;

        Ok(supported_operations_message)
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
        let needs_cloud_update = self
            .supported_operations
            .load_from_dir(&message.ops_dir, &self.config.bridge_config)?;

        if needs_cloud_update {
            let device_xid = self.supported_operations.xid_from_path(&message.ops_dir)?;
            Ok(Some(
                self.supported_operations.create_supported_operations(
                    &device_xid,
                    &self.config.bridge_config.c8y_prefix,
                )?,
            ))
        } else {
            Ok(None)
        }
    }
}

pub fn create_get_pending_operations_message(prefix: &TopicPrefix) -> MqttMessage {
    let topic = C8yTopic::upstream_topic(prefix);
    MqttMessage::new(&topic, request_pending_operations())
}

impl CumulocityConverter {
    /// Register on C8y an operation capability for a device.
    ///
    /// Additionally when the target is a child device, operation directory for the device will be loaded and operations
    /// not already registered will be registered.
    ///
    /// Returns a Set Supported Operations (114) message if among registered operations there were new operations that
    /// were not announced to the cloud.
    pub async fn register_operation(
        &mut self,
        target: &EntityTopicId,
        c8y_operation_name: &str,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let device = self.entity_cache.try_get(target)?;

        self.supported_operations
            .add_operation(device.external_id.as_ref(), c8y_operation_name)
            .await?;

        let need_cloud_update = match device.metadata.r#type {
            // for devices other than the main device and services, dynamic update of supported operations via file events is
            // disabled, so we have to additionally load new operations from the c8y operations for that device
            EntityType::ChildDevice | EntityType::Service => self
                .supported_operations
                .load_all(device.external_id.as_ref(), &self.config.bridge_config)?,

            // for main devices new operation files are loaded dynamically as they are created, so only register one
            // operation we need
            EntityType::MainDevice => self.supported_operations.load(
                device.external_id.as_ref(),
                c8y_operation_name,
                &self.config.bridge_config,
            )?,
        };

        if need_cloud_update {
            let cloud_update_operations_message =
                self.supported_operations.create_supported_operations(
                    device.external_id.as_ref(),
                    &self.config.bridge_config.c8y_prefix,
                )?;

            return Ok(vec![cloud_update_operations_message]);
        }

        Ok(vec![])
    }

    async fn register_restart_operation(
        &mut self,
        target: &EntityTopicId,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if !self.config.capabilities.device_restart {
            warn!("Received restart metadata, however, device restart feature is disabled");
            return Ok(vec![]);
        }

        match self.register_operation(target, "c8y_Restart").await {
            Err(_) => {
                error!("Fail to register `restart` operation for unknown device: {target}");
                Ok(vec![])
            }
            Ok(messages) => Ok(messages),
        }
    }

    async fn register_custom_operation(
        &mut self,
        target: &EntityTopicId,
        command_name: &str,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if let Some(c8y_op_name) = self
            .supported_operations
            .get_operation_name_by_workflow_operation(command_name)
        {
            match self.register_operation(target, &c8y_op_name).await {
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
        if !self.config.capabilities.software_update {
            warn!(
                "Received software update metadata, however, software update feature is disabled"
            );
            return Ok(vec![]);
        }
        let mut registration = match self.register_operation(target, "c8y_SoftwareUpdate").await {
            Err(_) => {
                error!("Fail to register `software-list` operation for unknown device: {target}");
                return Ok(vec![]);
            }
            Ok(messages) => messages,
        };

        registration.push(self.request_software_list(target));
        Ok(registration)
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
    use super::*;
    use crate::actor::IdDownloadRequest;
    use crate::actor::IdDownloadResult;
    use crate::actor::IdUploadRequest;
    use crate::actor::IdUploadResult;
    use crate::config::BridgeConfig;
    use crate::config::C8yMapperConfig;
    use crate::entity_cache::InvalidExternalIdError;
    use crate::supported_operations::operation::ResultFormat;
    use crate::supported_operations::SupportedOperations;
    use crate::tests::spawn_dummy_c8y_http_proxy;
    use crate::Capabilities;
    use anyhow::Result;
    use assert_json_diff::assert_json_include;
    use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
    use c8y_api::proxy_url::Protocol;
    use c8y_api::smartrest::topic::C8yTopic;
    use c8y_http_proxy::handle::C8YHttpProxy;
    use serde_json::json;
    use serde_json::Value;
    use std::str::FromStr;
    use std::time::Duration;
    use std::time::SystemTime;
    use tedge_actors::test_helpers::FakeServerBox;
    use tedge_actors::test_helpers::FakeServerBoxBuilder;
    use tedge_actors::Builder;
    use tedge_actors::ClientMessageBox;
    use tedge_actors::CloneSender;
    use tedge_actors::LoggingSender;
    use tedge_actors::SimpleMessageBoxBuilder;
    use tedge_api::mqtt_topics::default_topic_schema;
    use tedge_api::mqtt_topics::Channel;
    use tedge_api::mqtt_topics::ChannelFilter;
    use tedge_api::mqtt_topics::EntityFilter;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_api::mqtt_topics::OperationType;
    use tedge_api::pending_entity_store::RegisteredEntityData;
    use tedge_api::script::ShellScript;
    use tedge_api::SoftwareUpdateCommand;
    use tedge_config::models::AutoLogUpload;
    use tedge_config::models::SoftwareManagementApiFlag;
    use tedge_config::models::TopicPrefix;
    use tedge_config::TEdgeConfig;
    use tedge_flows::Message;
    use tedge_flows::Transformer;
    use tedge_http_ext::HttpRequest;
    use tedge_http_ext::HttpResult;
    use tedge_mqtt_ext::test_helpers::assert_messages_matching;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::QoS;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;

    #[tokio::test]
    async fn test_sync_alarms() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        let alarm_topic = "te/device/main///a/temperature_alarm";
        let alarm_payload = r#"{ "severity": "critical", "text": "Temperature very high" }"#;
        let alarm_message = MqttMessage::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

        // During the sync phase, alarms are not converted immediately, but only cached to be synced later
        assert!(converter.convert(&alarm_message).await.is_empty());

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

        // After the sync phase, the conversion of alarms is done immediately
        assert!(!converter.convert(alarm_message).await.is_empty());

        // But, even after the sync phase, internal alarms are not converted and just ignored, as they are purely internal
        assert!(converter.convert(&internal_alarm_message).await.is_empty());
    }

    #[tokio::test]
    async fn test_sync_child_alarms() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        let alarm_topic = "te/device/external_sensor///a/temperature_alarm";
        let alarm_payload = r#"{ "severity": "critical", "text": "Temperature very high" }"#;
        let alarm_message = MqttMessage::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

        register_source_entities(alarm_topic, &mut converter).await;

        // During the sync phase, alarms are not converted immediately, but only cached to be synced later
        assert!(converter.convert(&alarm_message).await.is_empty());

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

        // After the sync phase, the conversion of alarms is done immediately
        assert!(!converter.convert(alarm_message).await.is_empty());

        // But, even after the sync phase, internal alarms are not converted and just ignored, as they are purely internal
        assert!(converter.convert(&internal_alarm_message).await.is_empty());
    }

    #[tokio::test]
    async fn convert_child_device_registration() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        let in_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            json!({
                "@type":"child-device",
                "name":"child1"
            })
            .to_string(),
        );
        let UpdateOutcome::Inserted(entities) = converter
            .process_entity_metadata_message(&in_message)
            .await
            .unwrap()
        else {
            panic!("Expected insert outcome");
        };

        assert_eq!(entities.len(), 1);

        let messages = converter.convert(&in_message).await;

        assert_messages_matching(
            &messages,
            [(
                "c8y/s/us",
                "101,test-device:device:child1,child1,thin-edge.io-child,false".into(),
            )],
        );
    }

    #[tokio::test]
    async fn convert_child_device_registration_control_is_device_fragment() {
        let tmp_dir = TempTedgeDir::new();
        let mut config = c8y_converter_config(&tmp_dir);
        config.smartrest_child_device_create_with_device_marker = true;
        let (mut converter, _) = create_c8y_converter_from_config(config);

        let in_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            json!({
                "@type":"child-device",
                "name":"child1"
            })
            .to_string(),
        );
        let UpdateOutcome::Inserted(entities) = converter
            .process_entity_metadata_message(&in_message)
            .await
            .unwrap()
        else {
            panic!("Expected insert outcome");
        };

        assert_eq!(entities.len(), 1);

        let messages = converter.convert(&in_message).await;

        assert_messages_matching(
            &messages,
            [(
                "c8y/s/us",
                "101,test-device:device:child1,child1,thin-edge.io-child,true".into(),
            )],
        );
    }

    #[tokio::test]
    async fn convert_measurement_with_child_id() {
        let tmp_dir = TempTedgeDir::new();
        let mut converter = MeasurementConverter::new(&tmp_dir);

        let in_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///m/"),
            json!({
                "temp": 1,
                "time": "2021-11-16T17:45:40.571760714+01:00"
            })
            .to_string(),
        );

        converter
            .register_source_entities(&in_message.topic.name)
            .await;

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
        let mut converter = MeasurementConverter::new(&tmp_dir);
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
            .process_entity_metadata_message(&reg_message)
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
            .process_entity_metadata_message(&reg_message)
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
        let mut converter = MeasurementConverter::new(&tmp_dir);
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
            .process_entity_metadata_message(&reg_message)
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
            .process_entity_metadata_message(&reg_message)
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
            .process_entity_metadata_message(&reg_message)
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
        let mut converter = MeasurementConverter::new(&tmp_dir);

        let in_topic = "te/device/child1/service/app1/m/m_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        converter.register_source_entities(in_topic).await;

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
        let mut converter = MeasurementConverter::new(&tmp_dir);

        let in_topic = "te/device/main/service/appm/m/m_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        converter.register_source_entities(in_topic).await;

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
        let converter = MeasurementConverter::new(&tmp_dir);

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
            "101,test-device:device:child1,child1,thin-edge.io-child,false",
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
    async fn convert_two_measurement_messages_given_different_child_id() {
        let tmp_dir = TempTedgeDir::new();
        let mut converter = MeasurementConverter::new(&tmp_dir);
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;

        // First message from "child1"
        let in_first_message =
            MqttMessage::new(&Topic::new_unchecked("te/device/child1///m/"), in_payload);
        converter
            .register_source_entities(&in_first_message.topic.name)
            .await;

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
        converter
            .register_source_entities(&in_second_message.topic.name)
            .await;

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
        let mut converter = MeasurementConverter::new(&tmp_dir);

        let in_topic = "te/device/main///m/test_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        converter.register_source_entities(in_topic).await;

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
        let mut converter = MeasurementConverter::new(&tmp_dir);

        let in_topic = "te/device/main///m/test_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00","type":"type_in_payload"}"#;
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        converter.register_source_entities(in_topic).await;

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
        let mut converter = MeasurementConverter::new(&tmp_dir);

        let in_topic = "te/device/child///m/test_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        converter.register_source_entities(in_topic).await;

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
        let mut converter = MeasurementConverter::new(&tmp_dir);

        let in_topic = "te/device/child2///m/test_type";
        let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00","type":"type_in_payload"}"#;
        let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

        converter.register_source_entities(in_topic).await;

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
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

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
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
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
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
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
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
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
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
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
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
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
        let (mut converter, http_proxy) = create_c8y_converter(&tmp_dir);
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
        let (mut converter, http_proxy) = create_c8y_converter(&tmp_dir);
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
        let converter = MeasurementConverter::new(&tmp_dir);
        let measurement_topic = "te/device/main///m/";
        let big_measurement_payload = create_thin_edge_measurement(10 * 1024); // Measurement payload > size_threshold after converting to c8y json

        let big_measurement_message = MqttMessage::new(
            &Topic::new_unchecked(measurement_topic),
            big_measurement_payload,
        );
        let result = converter.convert(&big_measurement_message).await;

        let payload = result[0].payload_str().unwrap();
        assert!(payload.contains(r#""temperature0":{"temperature0":{"value":0.0}}"#));
    }

    #[tokio::test]
    async fn test_convert_small_measurement() {
        let tmp_dir = TempTedgeDir::new();
        let converter = MeasurementConverter::new(&tmp_dir);
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
        let mut converter = MeasurementConverter::new(&tmp_dir);
        let measurement_topic = "te/device/child1///m/";
        let big_measurement_payload = create_thin_edge_measurement(10 * 1024); // Measurement payload > size_threshold after converting to c8y json

        let big_measurement_message = MqttMessage::new(
            &Topic::new_unchecked(measurement_topic),
            big_measurement_payload,
        );

        converter.register_source_entities(measurement_topic).await;

        let result = converter.convert(&big_measurement_message).await;

        // Skipping the first two auto-registration messages and validating the third mapped message
        let payload = result[0].payload_str().unwrap();
        assert!(payload.contains(r#""temperature0":{"temperature0":{"value":0.0}}"#));
    }

    #[tokio::test]
    async fn test_execute_operation_is_not_blocked() {
        let tmp_dir = TempTedgeDir::new();
        let (converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        let now = Instant::now();
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
    async fn operations_are_deduplicated() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        let operation = MqttMessage::new(&Topic::new_unchecked("c8y/devicecontrol/notifications"), json!(
            {"id":"16574089","status":"PENDING","c8y_Restart":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());
        assert_eq!(
            converter.try_convert(&operation).await.unwrap(),
            vec![MqttMessage {
                topic: Topic {
                    name: "te/device/main///cmd/restart/c8y-mapper-16574089".into(),
                },
                payload: json!({"status":"init"}).to_string().into(),
                qos: QoS::AtLeastOnce,
                retain: true,
            },]
        );
        assert_eq!(converter.try_convert(&operation).await.unwrap(), vec![]);
    }

    #[tokio::test]
    async fn operations_are_deduplicated_after_completion() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        let operation = MqttMessage::new(&Topic::new_unchecked("c8y/devicecontrol/notifications"), json!(
            {"id":"16574089","status":"PENDING","c8y_Restart":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());
        assert_eq!(
            converter.try_convert(&operation).await.unwrap(),
            vec![MqttMessage {
                topic: Topic {
                    name: "te/device/main///cmd/restart/c8y-mapper-16574089".into(),
                },
                payload: json!({"status":"init"}).to_string().into(),
                qos: QoS::AtLeastOnce,
                retain: true,
            },]
        );
        let local_completion = MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/restart/c8y-mapper-16574089"),
            "",
        );
        assert_eq!(
            converter.try_convert(&local_completion).await.unwrap(),
            vec![]
        );

        assert_eq!(converter.try_convert(&operation).await.unwrap(), vec![]);
    }

    #[tokio::test]
    async fn custom_operations_are_deduplicated() {
        let tmp_dir = TempTedgeDir::new();
        let custom_op = r#"exec.topic = "my/custom/topic"
        exec.on_fragment = "my_op"
        exec.command = "/etc/tedge/operations/command ${.payload.my_op.text}""#;
        let f = tmp_dir.dir("operations").dir("c8y").file("my_op");
        f.with_raw_content(custom_op);
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        let operation = MqttMessage::new(&Topic::new_unchecked("my/custom/topic"), json!(
            {"id":"16574089","status":"PENDING","my_op":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());
        assert!(
            matches!(
                dbg!(converter.try_convert(&operation).await.unwrap().as_slice()),
                [MqttMessage { topic, .. }, ..] if topic.to_string() == "c8y/s/us"
            ),
            "Initial operation delivery produces outgoing message"
        );
        assert_eq!(
            converter.try_convert(&operation).await.unwrap(),
            vec![],
            "Operation redelivery is ignored by converter"
        );
    }

    #[tokio::test]
    async fn custom_operations_are_not_deduplicated_before_registration() {
        // We could potentially receive a custom operation before we have
        // registered a handler for it. If we then register a suitable handler
        // and the operation is redelivered to the mapper, this should be
        // processed
        let tmp_dir = TempTedgeDir::new();
        let custom_op = r#"exec.topic = "my/custom/topic"
        exec.on_fragment = "my_op"
        exec.command = "/etc/tedge/operations/command ${.payload.my_op.text}""#;
        let f = tmp_dir.dir("operations").dir("c8y").file("my_op");
        f.with_raw_content(custom_op);
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        let before_registration = SupportedOperations {
            device_id: converter.supported_operations.device_id.clone(),
            base_ops_dir: converter.supported_operations.base_ops_dir.clone(),
            operations_by_xid: <_>::default(),
        };
        let after_registration =
            std::mem::replace(&mut converter.supported_operations, before_registration);

        let operation = MqttMessage::new(&Topic::new_unchecked("my/custom/topic"), json!(
            {"id":"16574089","status":"PENDING","my_op":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());
        assert_eq!(
            converter.try_convert(&operation).await.unwrap(),
            vec![],
            "Operation is ignored before the operation is registered"
        );

        converter.supported_operations = after_registration;

        assert!(
            matches!(
                dbg!(converter.try_convert(&operation).await.unwrap().as_slice()),
                [MqttMessage { topic, .. }, ..] if topic.to_string() == "c8y/s/us"
            ),
            "First delivery after registration produces outgoing message"
        );
    }

    #[tokio::test]
    async fn te_topic_operations_do_not_have_time_based_expiry() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        tokio::time::pause();

        let operation = MqttMessage::new(&Topic::new_unchecked("c8y/devicecontrol/notifications"), json!(
            {"id":"16574089","status":"PENDING","c8y_Restart":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());
        let expected_message = MqttMessage {
            topic: Topic {
                name: "te/device/main///cmd/restart/c8y-mapper-16574089".into(),
            },
            payload: json!({"status":"init"}).to_string().into(),
            qos: QoS::AtLeastOnce,
            retain: true,
        };

        assert_eq!(
            converter.try_convert(&operation).await.unwrap().first(),
            Some(&expected_message),
            "First delivery after registration produces outgoing message"
        );
        assert_eq!(converter.active_commands.len(), 1);

        // Converter should disable time-based expiry when the `te` topic
        // message is processed
        converter.try_convert(&expected_message).await.unwrap();

        tokio::time::advance(Duration::from_secs(24 * 3600)).await;

        let random_message = MqttMessage::new(&Topic::new_unchecked("c8y/s/ds"), "510,test");

        // Trigger the converter since it performs cache eviction only when it's converting c8y messages
        converter.try_convert(&random_message).await.unwrap();
        assert_eq!(converter.active_commands.len(), 1);
    }

    #[tokio::test]
    async fn custom_operation_ids_are_not_cached_indefinitely() {
        let tmp_dir = TempTedgeDir::new();
        let custom_op = r#"exec.topic = "my/custom/topic"
        exec.on_fragment = "my_op"
        exec.command = "/etc/tedge/operations/command ${.payload.my_op.text}""#;
        let f = tmp_dir.dir("operations").dir("c8y").file("my_op");
        f.with_raw_content(custom_op);

        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        tokio::time::pause();

        let operation = MqttMessage::new(&Topic::new_unchecked("my/custom/topic"), json!(
            {"id":"16574089","status":"PENDING","my_op":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());

        assert!(
            matches!(
                dbg!(converter.try_convert(&operation).await.unwrap().as_slice()),
                [MqttMessage { topic, .. }, ..] if topic.to_string() == "c8y/s/us"
            ),
            "First delivery after registration produces outgoing message"
        );

        assert_eq!(converter.active_commands.len(), 1);
        tokio::time::advance(Duration::from_secs(24 * 3600)).await;

        let random_message = MqttMessage::new(&Topic::new_unchecked("c8y/s/ds"), "510,test");

        // Trigger the converter since it performs cache eviction only when it's converting c8y messages
        converter.try_convert(&random_message).await.unwrap();
        assert_eq!(converter.active_commands.len(), 0);
    }

    #[tokio::test]
    async fn active_commands_is_populated_with_existing_commands_from_retain_messages() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        let existing_pending_operation = MqttMessage {
            topic: Topic {
                name: "te/device/main///cmd/restart/c8y-mapper-16574089".into(),
            },
            payload: json!({"status":"init"}).to_string().into(),
            qos: QoS::AtLeastOnce,
            retain: true,
        };
        converter
            .try_convert(&existing_pending_operation)
            .await
            .unwrap();

        let operation = MqttMessage::new(&Topic::new_unchecked("c8y/devicecontrol/notifications"), json!(
            {"id":"16574089","status":"PENDING","c8y_Restart":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());

        assert_eq!(
            converter.try_convert(&operation).await.unwrap().as_slice(),
            [],
            "Existing tedge operation should trigger de-duplication"
        );
    }

    #[tokio::test]
    async fn custom_operation_ids_are_not_evicted_from_cache_prematurely() {
        let tmp_dir = TempTedgeDir::new();
        let custom_op = r#"exec.topic = "my/custom/topic"
        exec.on_fragment = "my_op"
        exec.command = "/etc/tedge/operations/command ${.payload.my_op.text}""#;
        let f = tmp_dir.dir("operations").dir("c8y").file("my_op");
        f.with_raw_content(custom_op);

        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        tokio::time::pause();

        let operation = MqttMessage::new(&Topic::new_unchecked("my/custom/topic"), json!(
            {"id":"16574089","status":"PENDING","my_op":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());

        assert!(
            matches!(
                dbg!(converter.try_convert(&operation).await.unwrap().as_slice()),
                [MqttMessage { topic, .. }, ..] if topic.to_string() == "c8y/s/us"
            ),
            "First delivery after registration produces outgoing message"
        );

        assert_eq!(converter.active_commands.len(), 1);
        // After a minute, the operation id should still exist
        tokio::time::advance(Duration::from_secs(60)).await;

        let random_message = MqttMessage::new(&Topic::new_unchecked("c8y/s/ds"), "510,test");
        converter.try_convert(&random_message).await.unwrap();
        assert_eq!(converter.active_commands.len(), 1);
    }

    #[tokio::test]
    async fn handle_operations_for_child_device() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        // The child has first to declare its capabilities
        let mqtt_schema = MqttSchema::default();
        let child = EntityTopicId::default_child_device("childId").unwrap();
        let child_capability = SoftwareUpdateCommand::capability_message(&mqtt_schema, &child);

        register_source_entities(&child_capability.topic.name, &mut converter).await;

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
        let (converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        assert_eq!(
            converter.default_device_name_from_external_id(&external_id.into()),
            device_name
        );
    }

    #[tokio::test]
    async fn duplicate_registration_messages_not_mapped_2311() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        let in_topic = "te/device/main/service/my_measurement_service";
        register_source_entities(in_topic, &mut converter).await;

        let reg_message =
            MqttMessage::new(&Topic::new_unchecked(in_topic), r#"{"@type": "service"}"#);

        // when converting a registration message the same as the previous one, no additional registration messages should be produced
        let outcome = converter
            .process_entity_metadata_message(&reg_message)
            .await
            .unwrap();
        assert_eq!(outcome, UpdateOutcome::Unchanged);
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

        register_source_entities(&service_health_message.topic.name, &mut converter).await;

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

        let UpdateOutcome::Inserted(entities) = converter
            .process_entity_metadata_message(&reg_message)
            .await
            .unwrap()
        else {
            panic!("Expected insert outcome");
        };

        let messages = registered_entities_into_mqtt_messages(entities);

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

        let outcome = converter
            .process_entity_metadata_message(&reg_message)
            .await
            .unwrap();
        assert_eq!(outcome, UpdateOutcome::Unchanged);

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

        let outcome = converter
            .process_entity_metadata_message(&reg_message)
            .await
            .unwrap();
        assert_eq!(outcome, UpdateOutcome::Unchanged);

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
        let UpdateOutcome::Inserted(entities) = converter
            .process_entity_metadata_message(&reg_message)
            .await
            .unwrap()
        else {
            panic!("Expected insert outcome");
        };
        let messages = registered_entities_into_mqtt_messages(entities);
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
        let main_device = EntityTopicId::default_main_device();

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
            .process_entity_metadata_message(&reg_message)
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
            .process_entity_metadata_message(&reg_message)
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
        assert_messages_matching(&msgs, [("c8y/s/us/service0", "114,c8y_Operation".into())]);
    }

    fn registered_entities_into_mqtt_messages(
        entities: Vec<RegisteredEntityData>,
    ) -> Vec<MqttMessage> {
        let mut messages = vec![];
        for entity in entities {
            messages.push(entity.reg_message.to_mqtt_message(&MqttSchema::default()));
            for data_message in entity.data_messages {
                messages.push(data_message);
            }
        }
        messages
    }

    pub(crate) fn create_c8y_converter(
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

        let flow_context = FlowContextHandle::default();
        let converter = CumulocityConverter::new(
            config,
            mqtt_publisher,
            http_proxy,
            uploader,
            downloader,
            flow_context,
        )
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

    pub(crate) async fn register_source_entities(topic: &str, converter: &mut CumulocityConverter) {
        let (topic_id, _) = converter.mqtt_schema.entity_channel_of(topic).unwrap();
        let entities = default_topic_schema::parse(&topic_id);

        for entity in entities {
            let message = entity.to_mqtt_message(&converter.mqtt_schema);
            converter
                .process_entity_metadata_message(&message)
                .await
                .unwrap();
        }
    }

    struct MeasurementConverter {
        c8y_converter: CumulocityConverter,
        measurement_converter: crate::mea::measurements::MeasurementConverter,
        _http: FakeServerBox<HttpRequest, HttpResult>,
    }

    impl MeasurementConverter {
        fn new(tmp_dir: &TempTedgeDir) -> Self {
            let (c8y_converter, _http) = create_c8y_converter(tmp_dir);
            MeasurementConverter {
                c8y_converter,
                measurement_converter: Default::default(),
                _http,
            }
        }

        async fn register_source_entities(&mut self, topic: &str) {
            register_source_entities(topic, &mut self.c8y_converter).await
        }

        async fn process_entity_metadata_message(
            &mut self,
            message: &MqttMessage,
        ) -> Result<UpdateOutcome, ConversionError> {
            self.c8y_converter
                .process_entity_metadata_message(message)
                .await
        }

        async fn convert(&self, message: &MqttMessage) -> Vec<MqttMessage> {
            let context = self.c8y_converter.entity_cache.flow_context();
            let timestamp = SystemTime::now();
            let message: Message = message.clone().into();
            match self
                .measurement_converter
                .on_message(timestamp, &message, context)
            {
                Ok(messages) => messages
                    .into_iter()
                    .filter_map(|msg| MqttMessage::try_from(msg).ok())
                    .collect(),
                Err(_) => {
                    vec![]
                }
            }
        }
    }
}
