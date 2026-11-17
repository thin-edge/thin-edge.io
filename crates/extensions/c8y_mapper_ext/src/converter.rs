use super::config::C8yMapperConfig;
use super::error::CumulocityMapperError;
use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::command_tracker::CommandTracker;
use crate::dynamic_discovery::DiscoverOp;
use crate::entity_cache::EntityCache;
use crate::entity_cache::InvalidExternalIdError;
use crate::entity_cache::UpdateOutcome;
use crate::error::ConversionError;
use crate::error::MessageConversionError;
use crate::mea::events::EventConverter;
use crate::operations;
use crate::operations::OperationHandler;
use crate::shell_execute::SHELL_EXECUTE_OPERATION;
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
use c8y_api::json_c8y_deserializer::C8yOperation;
use c8y_api::json_c8y_deserializer::C8ySoftwareUpdate;
use c8y_api::smartrest::inventory::child_device_creation_message;
use c8y_api::smartrest::inventory::service_creation_message;
use c8y_api::smartrest::message::collect_smartrest_messages;
use c8y_api::smartrest::message::get_failure_reason_for_smartrest;
use c8y_api::smartrest::message::get_smartrest_device_id;
use c8y_api::smartrest::message::get_smartrest_template_id;
use c8y_api::smartrest::message::MAX_PAYLOAD_LIMIT_IN_BYTES;
use c8y_api::smartrest::payload::SmartrestPayload;
use c8y_api::smartrest::smartrest_serializer::request_pending_operations;
use c8y_api::smartrest::smartrest_serializer::EmbeddedCsv;
use c8y_api::smartrest::smartrest_serializer::OperationReference;
use c8y_api::smartrest::smartrest_serializer::TextOrCsv;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::CreateEvent;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Output;
use std::str::FromStr;
use std::sync::Arc;
use tedge_actors::ClientMessageBox;
use tedge_actors::LoggingSender;
use tedge_actors::Sender;
use tedge_api::commands::RestartCommand;
use tedge_api::commands::SoftwareCommandMetadata;
use tedge_api::commands::SoftwareListCommand;
use tedge_api::control::ActionName;
use tedge_api::entity::EntityExternalId;
use tedge_api::entity::EntityType;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::IdGenerator;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::script::ShellScript;
use tedge_api::workflow::log::log_dir::OperationLogsError;
use tedge_api::workflow::GenericCommandState;
use tedge_api::DownloadInfo;
use tedge_api::Jsonify;
use tedge_api::LoggedCommand;
use tedge_config::models::TopicPrefix;
use tedge_config::TEdgeConfigError;
use tedge_flows::FlowContextHandle;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use tedge_uploader_ext::UploadRequest;
use tedge_uploader_ext::UploadResult;
use tedge_utils::file::FileError;
use thiserror::Error;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::trace;
use tracing::warn;
use tracing::Instrument;

const FORBIDDEN_ID_CHARS: [char; 3] = ['/', '+', '#'];
const EARLY_MESSAGE_BUFFER_SIZE: usize = 100;

#[derive(Debug)]
pub struct MapperConfig {
    pub http_event_topic: TopicFilter,
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
    pub config: Arc<C8yMapperConfig>,
    pub(crate) mapper_config: MapperConfig,
    pub device_name: String,
    mqtt_publisher: LoggingSender<MqttMessage>,
    pub http_proxy: C8YHttpProxy,
    pub service_type: String,
    pub mqtt_schema: MqttSchema,
    pub(crate) entity_cache: EntityCache,

    pub command_id: IdGenerator,
    pub(crate) commands: CommandTracker,

    pub supported_operations: SupportedOperations,
    pub operation_handler: OperationHandler,
    // Keep the service actions declared by the services of this device and its children
    pub(crate) service_actions: HashMap<EntityTopicId, Vec<ActionName>>,
}

impl CumulocityConverter {
    pub fn new(
        config: C8yMapperConfig,
        mqtt_publisher: LoggingSender<MqttMessage>,
        http_proxy: C8YHttpProxy,
        uploader: ClientMessageBox<(String, UploadRequest), (String, UploadResult)>,
        downloader: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
        http_client: ClientMessageBox<HttpRequest, HttpResult>,
        flow_context: FlowContextHandle,
    ) -> Result<Self, CumulocityConverterBuildError> {
        let device_id = config.device_id.clone();

        let service_type = if config.service.ty.is_empty() {
            "service".to_owned()
        } else {
            config.service.ty.clone()
        };

        let ops_dir = config.ops_dir.path();
        let operations_by_xid = {
            let mut operations = get_child_ops(ops_dir, &config.bridge_config)?;
            operations.insert(
                config.device_id.clone(),
                Operations::try_new(ops_dir, &config.bridge_config)?,
            );
            operations
        };
        let operation_manager = SupportedOperations {
            device_id: device_id.clone(),

            base_ops_dir: Arc::clone(&config.ops_dir),

            operations_by_xid,
        };

        let mqtt_schema = config.mqtt_schema.clone();
        let http_event_topic =
            EventConverter::http_event_topic_filter(&config.bridge_config.c8y_prefix);
        let mapper_config = MapperConfig {
            http_event_topic,
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
            http_client,
        );

        Ok(CumulocityConverter {
            config: Arc::new(config),
            mapper_config,
            device_name: device_id,
            supported_operations: operation_manager,
            http_proxy,
            mqtt_publisher,
            service_type,
            mqtt_schema: mqtt_schema.clone(),
            entity_cache,
            command_id,
            commands: CommandTracker::default(),
            operation_handler,
            service_actions: HashMap::new(),
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
            self.service_actions.remove(&topic_id);
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
            &entity.r#type(),
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

    async fn post_event_over_http(&mut self, message: &MqttMessage) -> Result<(), ConversionError> {
        let c8y_event = C8yCreateEvent::from_slice(message.payload_bytes())?;
        let device_id = message
            .topic
            .as_ref()
            .rsplit_once('/')
            .map(|(_, id)| id.to_string())
            .unwrap_or_else(|| {
                self.entity_cache
                    .main_device_external_id()
                    .as_ref()
                    .to_string()
            });
        let create_event = CreateEvent {
            event_type: c8y_event.event_type,
            time: c8y_event.time,
            text: c8y_event.text,
            extras: c8y_event.extras,
            device_id,
        };
        self.http_proxy.send_event(create_event).await?;
        Ok(())
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

            if self.commands.is_known(&cmd_id) {
                info!("{cmd_id} is already addressed");
                return Ok(vec![]);
            }

            // wrap operation payload in a dummy MqttMessage wrapper because the code below assumes 1 MQTT message = 1 operation
            // TODO: refactor to avoid this intermediate step and extra copies
            let operation_message = MqttMessage::new(&message.topic, operation_payload);

            let result = self
                .process_json_over_mqtt(
                    device_xid.clone(),
                    operation.op_id.clone(),
                    &operation.extras,
                    &operation_message,
                )
                .await;
            let result = self.handle_c8y_operation_result(
                &result,
                &device_xid,
                Some(operation.op_id.clone()),
            );
            self.commands.started(cmd_id);
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

        let operation = C8yDeviceControlOperation::from_json_object(extras)?;
        if let Some((c8y_operation, feature)) = self.disabled_feature_for(&operation) {
            warn!("Received a {c8y_operation} operation, however, {feature} feature is disabled");
            return Ok(vec![]);
        }

        let msgs = match operation {
            C8yDeviceControlOperation::Restart(_) => {
                self.forward_restart_request(device_xid, cmd_id)?
            }
            C8yDeviceControlOperation::SoftwareUpdate(request) => {
                self.forward_software_request(device_xid, cmd_id, request)
                    .await?
            }
            C8yDeviceControlOperation::LogfileRequest(request) => {
                self.convert_log_upload_request(device_xid, cmd_id, request)?
            }
            C8yDeviceControlOperation::UploadConfigFile(request) => {
                self.convert_config_snapshot_request(device_xid, cmd_id, request)?
            }
            C8yDeviceControlOperation::DownloadConfigFile(request) => {
                self.convert_config_update_request(device_xid, cmd_id, request)
                    .await?
            }
            C8yDeviceControlOperation::Firmware(request) => {
                self.convert_firmware_update_request(device_xid, cmd_id, request)?
            }
            C8yDeviceControlOperation::DeviceProfile(request) => match extras.get("profileName") {
                Some(profile_name) => self.convert_device_profile_request(
                    device_xid,
                    cmd_id,
                    request,
                    serde_json::from_value(profile_name.clone())?,
                )?,
                None => {
                    error!("Received a c8y_DeviceProfile without a profile name");
                    vec![]
                }
            },
            C8yDeviceControlOperation::ServiceCommand(request) => {
                self.convert_control_request(device_xid, cmd_id, request)?
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

    /// The Cumulocity operation name and the feature it requires, if that feature is disabled
    fn disabled_feature_for(
        &self,
        operation: &C8yDeviceControlOperation,
    ) -> Option<(&'static str, &'static str)> {
        let capabilities = &self.config.capabilities;
        let (c8y_operation, feature, enabled) = match operation {
            C8yDeviceControlOperation::Restart(_) => {
                ("c8y_Restart", "device_restart", capabilities.device_restart)
            }
            C8yDeviceControlOperation::SoftwareUpdate(_) => (
                "c8y_SoftwareUpdate",
                "software_update",
                capabilities.software_update,
            ),
            C8yDeviceControlOperation::LogfileRequest(_) => {
                ("c8y_LogfileRequest", "log_upload", capabilities.log_upload)
            }
            C8yDeviceControlOperation::UploadConfigFile(_) => (
                "c8y_UploadConfigFile",
                "config_snapshot",
                capabilities.config_snapshot,
            ),
            C8yDeviceControlOperation::DownloadConfigFile(_) => (
                "c8y_DownloadConfigFile",
                "config_update",
                capabilities.config_update,
            ),
            C8yDeviceControlOperation::Firmware(_) => (
                "c8y_Firmware",
                "firmware_update",
                capabilities.firmware_update,
            ),
            C8yDeviceControlOperation::DeviceProfile(_) => (
                "c8y_DeviceProfile",
                "device_profile",
                capabilities.device_profile,
            ),
            C8yDeviceControlOperation::ServiceCommand(_) => (
                "c8y_ServiceCommand",
                "service_control",
                capabilities.service_control,
            ),
            C8yDeviceControlOperation::Custom => return None,
        };
        (!enabled).then_some((c8y_operation, feature))
    }

    async fn parse_json_custom_operation_topic(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let operation = C8yOperation::from_json(message.payload.as_str()?)?;
        let cmd_id = self.command_id.new_id_with_str(&operation.op_id);
        let device_xid = operation.external_source.external_id;

        if self.commands.is_known(&cmd_id) {
            info!("{cmd_id} is already addressed");
            return Ok(vec![]);
        }

        let result = self
            .process_json_custom_operation(
                operation.op_id.clone(),
                cmd_id.clone(),
                device_xid.clone(),
                &operation.extras,
                message,
            )
            .await;

        let output = self.handle_c8y_operation_result(&result, &device_xid, Some(operation.op_id));
        self.commands.started(cmd_id);

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

        let Some((on_fragment, custom_handler)) = handlers
            .iter()
            .find(|(on_fragment, _)| extras.contains_key(on_fragment))
        else {
            return Ok(vec![]);
        };
        match custom_handler.workflow_operation() {
            Some(command_name) if self.is_disabled_command(command_name) => {
                warn!("Received a {on_fragment} operation, however, the shell_execute feature is disabled");
            }
            Some(command_name) => {
                return self.convert_custom_operation_request(
                    device_xid,
                    cmd_id,
                    command_name.to_string(),
                    custom_handler,
                    message,
                );
            }
            None => {
                self.execute_custom_operation(custom_handler, message, &operation_id)
                    .await?;
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

        self.execute_operation(script, custom_handler, Some(operation_id.into()))
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
            let mut msgs = self.handle_c8y_operation_result(
                &result,
                self.entity_cache.main_device_external_id().as_ref(),
                None,
            );
            output.append(&mut msgs)
        }
        Ok(output)
    }

    fn handle_c8y_operation_result(
        &self,
        result: &Result<Vec<MqttMessage>, CumulocityMapperError>,
        target_xid: &str,
        op_id: Option<String>,
    ) -> Vec<MqttMessage> {
        match result {
            Err(err) => {
                error!("{err}");
                let Some(operation) = err.failed_operation_name() else {
                    return vec![];
                };

                let prefix = &self.config.bridge_config.c8y_prefix;
                let topic = if target_xid == self.entity_cache.main_device_external_id().as_ref() {
                    Ok(C8yTopic::upstream_topic(prefix))
                } else {
                    C8yTopic::ChildSmartRestResponse(target_xid.to_string()).to_topic(prefix)
                };
                let Ok(topic) = topic else {
                    error!("Not reporting the failure to Cumulocity: '{target_xid}' names no SmartREST topic");
                    return vec![];
                };

                let c8y_operation = OperationReference::new(
                    op_id,
                    self.config.smartrest_use_operation_id,
                    operation.to_owned(),
                );
                vec![
                    MqttMessage::new(&topic, c8y_operation.executing()),
                    MqttMessage::new(&topic, c8y_operation.failed(&err.to_string())),
                ]
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
        let mut command =
            software_update_request.into_software_update_command(target.topic_id(), cmd_id)?;

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
        let command = RestartCommand::new(target.topic_id(), cmd_id);
        let message = command.command_message(&self.mqtt_schema);
        Ok(vec![message])
    }

    pub fn has_deferred_software_list_requests(&self) -> bool {
        self.commands.has_deferred_software_list_requests()
    }

    /// Requests a software list from each target registered for software update,
    /// unless a request issued by this mapper is still pending for that target
    pub fn take_software_list_requests(&mut self) -> Vec<MqttMessage> {
        self.commands
            .take_deferred_software_list_requests()
            .iter()
            .map(|target| self.request_software_list(target))
            .collect()
    }

    fn request_software_list(&mut self, target: &EntityTopicId) -> MqttMessage {
        let cmd_id = self.command_id.new_id();
        let request = SoftwareListCommand::new(target, cmd_id.clone());
        self.commands.software_list_requested(cmd_id, target);
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
                self.execute_operation(script, operation, None).await?;
            }
        }
        // MQTT messages will be sent during the operation execution
        Ok(vec![])
    }

    async fn execute_operation(
        &self,
        script: ShellScript,
        operation: &Operation,
        operation_id: Option<String>,
    ) -> Result<(), CumulocityMapperError> {
        let operation_name = operation.name.clone();
        let execute_failed = |error_message: String| CumulocityMapperError::ExecuteFailed {
            error_message,
            command: script.command.clone(),
            operation_name: operation_name.clone(),
        };

        let tmp_dir = self.config.tmp_dir.root();
        if !tmp_dir.exists() {
            return Err(execute_failed(format!(
                "the configured tmp.path '{tmp_dir}' does not exist"
            )));
        }

        let mut logged = LoggedCommand::new(&script.command, tmp_dir)
            .map_err(|e| execute_failed(e.to_string()))?;
        if let Some(profile) = &self.config.cloud_profile {
            logged.env("TEDGE_CLOUD_PROFILE", profile);
        }
        logged.args(&script.args);
        let child_process = logged
            .spawn()
            .map_err(|e| execute_failed(e.to_string()))?
            .with_exit_wait_after_kill(self.config.custom_operation_exit_wait_after_kill);

        let mut command_log = self
            .config
            .logs_path
            .new_log_file(operation_name.clone(), self.command_id.new_id())
            .await?;

        let c8y_operation = OperationReference::new(
            operation_id,
            self.config.smartrest_use_operation_id,
            operation_name.clone(),
        );
        let skip_status_update = operation.skip_status_update();
        let mut mqtt_publisher = self.mqtt_publisher.clone();
        let topic = C8yTopic::upstream_topic(&self.config.bridge_config.c8y_prefix);
        let result_format = operation.result_format();
        let graceful_timeout = operation.graceful_timeout();
        let forceful_timeout = operation.forceful_timeout();

        tokio::spawn(
            async move {
                if !skip_status_update {
                    publish_status(&mut mqtt_publisher, &topic, c8y_operation.executing()).await;
                }

                let outcome = child_process
                    .wait_for_output_with_timeout(
                        &mut command_log,
                        graceful_timeout,
                        forceful_timeout,
                    )
                    .await;
                if let Err(err) = &outcome {
                    error!("Failed to run the {operation_name} operation: {err}");
                }

                // Built even when not published, to log an output that cannot be reported
                let status = final_status(&c8y_operation, outcome, result_format);
                if !skip_status_update {
                    publish_status(&mut mqtt_publisher, &topic, status).await;
                }
            }
            .instrument(tracing::Span::current()),
        );
        Ok(())
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

        let entity_type = self.entity_cache.try_get(&source)?.r#type();
        match &channel {
            Channel::EntityTwinData { fragment_key } => {
                self.try_convert_entity_twin_data(&source, &entity_type, message, fragment_key)
            }

            Channel::Measurement { .. } => Ok(vec![]),
            Channel::Event { .. } => Ok(vec![]),
            Channel::Alarm { .. } => Ok(vec![]),

            Channel::Command { cmd_id, .. } if message.payload_bytes().is_empty() => {
                // The command has been fully processed
                self.commands.completed(cmd_id);
                Ok(vec![])
            }

            Channel::CommandMetadata { operation } => {
                if entity_type == EntityType::Service && operation == &OperationType::Control {
                    return self.convert_control_metadata(&source, message).await;
                }

                // https://github.com/thin-edge/thin-edge.io/issues/2739
                if message.payload().is_empty() {
                    warn!(topic = ?message.topic.name, "Ignoring command metadata clearing message: clearing device capabilities is not currently supported");
                    return Ok(vec![]);
                }
                match operation {
                    OperationType::Restart => self.register_restart_operation(&source).await,
                    OperationType::SoftwareList => {
                        self.register_software_list_operation(&source, message)
                            .await
                    }
                    OperationType::SoftwareUpdate => {
                        self.register_software_update_operation(&source, message)
                            .await
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

            Channel::Command { operation, cmd_id } if self.command_id.is_generator_of(cmd_id) => {
                self.commands.resumed(cmd_id, operation, &source);

                let entity = self.entity_cache.try_get(&source)?;
                let entity = operations::EntityTarget {
                    topic_id: entity.topic_id().clone(),
                    external_id: entity.external_id.clone(),
                    smartrest_publish_topic: self
                        .smartrest_publish_topic_for_entity(entity.topic_id())?,
                };

                self.operation_handler.handle(entity, message.clone()).await;
                Ok(vec![])
            }

            Channel::Signal { signal_type } => self.process_signal_message(&source, signal_type),

            Channel::Health => Ok(vec![]),

            _ => Ok(vec![]),
        }
    }

    async fn try_convert_tedge_and_c8y_topics(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        self.commands.expire();

        let messages = match &message.topic {
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
            topic
                if topic.name
                    == C8yTopic::SmartRestRequest
                        .with_prefix(&self.config.bridge_config.c8y_prefix) =>
            {
                // We receive SmartREST operation notifications regardless of whether any custom
                // operations are configured to use it. This branch exists to prevent us logging
                // an error for this message we expect
                Ok(vec![])
            }
            topic if self.mapper_config.http_event_topic.accept_topic(topic) => {
                self.post_event_over_http(message).await?;
                Ok(vec![])
            }
            _ => {
                warn!(
                    "Received message on subscribed topic '{}' but no handler matched.",
                    message.topic.name
                );
                Ok(vec![])
            }
        }?;

        Ok(messages)
    }

    fn try_init_messages(&mut self) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut messages = vec![];

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
        // FIXME clean-up alarm sync on c8y actor
        vec![]
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

/// The status update reporting how an operation's command ended
fn final_status(
    c8y_operation: &OperationReference,
    outcome: Result<Output, std::io::Error>,
    result_format: ResultFormat,
) -> SmartrestPayload {
    let output = match outcome {
        Ok(output) => output,
        Err(err) => return c8y_operation.failed(&err.to_string()),
    };
    if !output.status.success() {
        let reason = get_failure_reason_for_smartrest(&output.stderr, MAX_PAYLOAD_LIMIT_IN_BYTES);
        return c8y_operation.failed(&reason);
    }

    // Not truncated here, as the characters rejected by Cumulocity
    // are removed before the result is truncated to fit the message
    let stdout = String::from_utf8(output.stdout).unwrap_or_else(|err| {
        error!("The output contains invalid UTF-8: {err}");
        String::new()
    });
    let result = match result_format {
        ResultFormat::Text => TextOrCsv::Text(stdout),
        ResultFormat::Csv => EmbeddedCsv::new(stdout).into(),
    };
    c8y_operation.succeeded(result).unwrap_or_else(|err| {
        let reason = format!(
            "{:?}",
            anyhow::Error::from(err).context(
                "Custom operation process exited successfully, but couldn't convert output to valid SmartREST message"
            )
        );
        c8y_operation.failed(&reason)
    })
}

async fn publish_status(
    mqtt_publisher: &mut LoggingSender<MqttMessage>,
    topic: &Topic,
    status: SmartrestPayload,
) {
    let message = MqttMessage::new(topic, status.as_str());
    if let Err(err) = mqtt_publisher.send(message).await {
        error!("Failed to publish a message: {status}. Error: {err}");
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

        let need_cloud_update = match device.r#type() {
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

    /// The built-in `shell_execute` command is only mapped to Cumulocity
    /// when the `c8y.enable.shell_execute` feature is enabled
    fn is_disabled_command(&self, command_name: &str) -> bool {
        command_name == SHELL_EXECUTE_OPERATION && !self.config.capabilities.shell_execute
    }

    async fn register_custom_operation(
        &mut self,
        target: &EntityTopicId,
        command_name: &str,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if self.is_disabled_command(command_name) {
            warn!(
                "Received {command_name} metadata, however, the shell_execute feature is disabled"
            );
            return Ok(vec![]);
        }

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
        message: &MqttMessage,
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

        if message.retain {
            self.commands.defer_software_list_request(target);
        } else {
            // A live capability message is published by an agent that has just (re)started
            self.commands.reset_software_list_requests(target);
            registration.push(self.request_software_list(target));
        }
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
pub(crate) mod tests;
