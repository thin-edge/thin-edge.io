//! The conversion of the messages received by the Cumulocity mapper
//!
//! The [`C8yMapperActor`](crate::actor::C8yMapperActor) registers the entities,
//! passes every other MQTT message it receives to
//! [`CumulocityConverter::convert`] and publishes the messages that returns.
//!
//! On thin-edge topics, the converter handles:
//! - entity registrations and twin data, which update the Cumulocity inventory
//! - command metadata, which declares a capability of an entity as a Cumulocity
//!   supported operation
//! - the commands created by this mapper, which are passed to the
//!   `OperationHandler` to report their progress to Cumulocity and to clear
//!   them once completed
//!
//! Measurements, events, alarms and health statuses are converted by flows,
//! except that events too large for MQTT come back to the converter to be
//! posted over HTTP.
//!
//! On Cumulocity topics, the converter handles operations, as defined by the
//! operation files loaded in `SupportedOperations`:
//! - built-in operations, and custom operations with a workflow, are converted
//!   into thin-edge commands
//! - custom operations without a workflow are run by the mapper itself,
//!   executing the command of their operation file, see `script_operations`
//!
//! The `CommandTracker` remembers the commands created for Cumulocity
//! operations, so that an operation received twice creates a single command.

use super::config::C8yMapperConfig;
use super::error::CumulocityMapperError;
use crate::actor::CmdId;
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
use c8y_api::smartrest::smartrest_serializer::request_pending_operations;
use c8y_api::smartrest::smartrest_serializer::OperationReference;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::CreateEvent;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tedge_actors::ClientMessageBox;
use tedge_actors::LoggingSender;
use tedge_api::commands::RestartCommand;
use tedge_api::commands::SoftwareCommandMetadata;
use tedge_api::commands::SoftwareListCommand;
use tedge_api::control::ActionName;
use tedge_api::entity::EntityExternalId;
use tedge_api::entity::EntityType;
use tedge_api::entity::TwinDefaultsConfig;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::IdGenerator;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::log::log_dir::OperationLogsError;
use tedge_api::DownloadInfo;
use tedge_api::Jsonify;
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

mod script_operations;

const FORBIDDEN_ID_CHARS: [char; 3] = ['/', '+', '#'];
const EARLY_MESSAGE_BUFFER_SIZE: usize = 100;

/// Converts the messages received by the Cumulocity mapper, see the
/// [module documentation](self)
pub struct CumulocityConverter {
    pub config: Arc<C8yMapperConfig>,
    http_event_topic: TopicFilter,
    errors_topic: Topic,
    mqtt_publisher: LoggingSender<MqttMessage>,
    pub http_proxy: C8YHttpProxy,
    pub mqtt_schema: MqttSchema,
    pub(crate) entity_cache: EntityCache,

    pub command_id: IdGenerator,
    /// The commands created by this mapper, see `CommandTracker`
    pub(crate) commands: CommandTracker,

    pub supported_operations: SupportedOperations,
    pub operation_handler: OperationHandler,
    /// The actions declared by each service of this device and its children
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

        let operation_manager = SupportedOperations::load_all_devices(
            device_id.clone(),
            Arc::clone(&config.ops_dir),
            &config.bridge_config,
        )?;

        let mqtt_schema = config.mqtt_schema.clone();
        let http_event_topic =
            EventConverter::http_event_topic_filter(&config.bridge_config.c8y_prefix);
        let errors_topic = mqtt_schema.error_topic();

        let entity_cache = EntityCache::new(
            flow_context,
            mqtt_schema.clone(),
            EntityTopicId::default_main_device(),
            device_id.into(),
            Self::map_to_c8y_external_id,
            Self::validate_external_id,
            EARLY_MESSAGE_BUFFER_SIZE,
            TwinDefaultsConfig::new(&config.service.ty),
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
            http_event_topic,
            errors_topic,
            supported_operations: operation_manager,
            http_proxy,
            mqtt_publisher,
            mqtt_schema,
            entity_cache,
            command_id,
            commands: CommandTracker::default(),
            operation_handler,
            service_actions: HashMap::new(),
        })
    }

    // Entry points, called by the actor

    /// Returns the messages to publish before any conversion: the main device's
    /// supported operations and a request for its pending operations
    pub fn init_messages(&mut self) -> Vec<MqttMessage> {
        self.try_init_messages()
            .unwrap_or_else(|error| vec![self.new_error_message(error)])
    }

    fn try_init_messages(&mut self) -> Result<Vec<MqttMessage>, ConversionError> {
        let main_device_xid = self.config.device_id.clone();
        Ok(vec![
            self.load_and_create_supported_operations_message(&main_device_xid)?,
            create_get_pending_operations_message(&self.config.bridge_config.c8y_prefix),
        ])
    }

    /// Converts a message received by the mapper into the messages to publish,
    /// replacing any error with a message on the errors topic
    pub async fn convert(&mut self, input: &MqttMessage) -> Vec<MqttMessage> {
        self.try_convert(input).await.unwrap_or_else(|error| {
            let topic = input.topic.name.clone();
            vec![self.new_error_message(MessageConversionError { error, topic })]
        })
    }

    pub async fn try_convert(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        debug!("Mapping message on topic: {}", message.topic.name);
        trace!("Message content: {:?}", message.payload_str());
        match self.mqtt_schema.entity_channel_of(&message.topic) {
            Ok((source, Channel::EntityMetadata)) => {
                self.convert_entity_registration(source, message)
            }
            Ok((source, channel)) => self.convert_data_message(source, channel, message).await,
            Err(_) => self.convert_c8y_message(message).await,
        }
    }

    /// Registers the entity described by a registration message, or deletes it on
    /// an empty payload. See `EntityCache::upsert` for how entities with an unknown
    /// parent are handled.
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
        Ok(self.entity_cache.upsert(register_message)?)
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

    pub fn process_operation_update_message(&mut self, message: DiscoverOp) -> Option<MqttMessage> {
        self.try_process_operation_update_message(&message)
            .unwrap_or_else(|err| Some(self.new_error_message(err)))
    }

    fn try_process_operation_update_message(
        &mut self,
        message: &DiscoverOp,
    ) -> Result<Option<MqttMessage>, ConversionError> {
        let needs_cloud_update = self
            .supported_operations
            .load_from_dir(&message.ops_dir, &self.config.bridge_config)?;
        if !needs_cloud_update {
            return Ok(None);
        }

        let device_xid = self.supported_operations.xid_from_path(&message.ops_dir)?;
        let prefix = &self.config.bridge_config.c8y_prefix;
        Ok(Some(
            self.supported_operations
                .create_supported_operations(&device_xid, prefix)?,
        ))
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

    // Messages received on thin-edge topics

    /// Converts an entity registration into the Cumulocity messages creating the
    /// entity and its twin data
    fn convert_entity_registration(
        &mut self,
        source: EntityTopicId,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let input = EntityRegistrationMessage::try_from(source, message.payload_bytes())?;

        let entity_topic_id = &input.topic_id;
        let entity = self.entity_cache.try_get(entity_topic_id)?;
        let external_id = &entity.external_id;
        let mut messages = vec![];
        match input.r#type {
            // Created by `tedge connect c8y`, or when the device first sends data
            EntityType::MainDevice => {}
            EntityType::ChildDevice => {
                let parent_xid: Option<&EntityExternalId> =
                    self.entity_cache.parent_external_id(entity_topic_id)?;

                messages.push(
                    child_device_creation_message(
                        external_id.as_ref(),
                        Some(entity.display_name()),
                        Some(entity.display_type()),
                        parent_xid.map(|xid| xid.as_ref()),
                        &self.config.device_id,
                        &self.config.bridge_config.c8y_prefix,
                        self.config.smartrest_child_device_create_with_device_marker,
                    )
                    .context("Could not create device creation message")?,
                );
            }
            EntityType::Service => {
                let parent_xid = self.entity_cache.parent_external_id(entity_topic_id)?;

                messages.push(
                    service_creation_message(
                        external_id.as_ref(),
                        entity.display_name(),
                        entity.display_type(),
                        "up",
                        parent_xid.map(|xid| xid.as_ref()),
                        &self.config.device_id,
                        &self.config.bridge_config.c8y_prefix,
                    )
                    .context("Could not create service creation message")?,
                );
            }
        }

        for (fragment_key, fragment_value) in input.twin_data.iter() {
            if fragment_key == "name" || fragment_key == "type" {
                // Skip converting the name and type fields as they are already included in the
                // registration message
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

    async fn convert_data_message(
        &mut self,
        source: EntityTopicId,
        channel: Channel,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if self.entity_cache.get(&source).is_none() {
            // On receipt of an unregistered entity data message, since it is received
            // before the entity itself is registered, cache it in the unregistered entity
            // store to be processed after the entity is registered
            self.entity_cache.cache_early_data_message(message.clone());
            return Ok(vec![]);
        }

        let entity_type = self.entity_cache.try_get(&source)?.r#type();
        match &channel {
            Channel::EntityTwinData { fragment_key } => {
                self.try_convert_entity_twin_data(&source, &entity_type, message, fragment_key)
            }

            Channel::Command { cmd_id, .. } if message.payload_bytes().is_empty() => {
                // The command has been fully processed
                self.commands.completed(cmd_id);
                Ok(vec![])
            }

            Channel::CommandMetadata {
                operation: OperationType::Control,
            } if entity_type == EntityType::Service => {
                self.convert_control_metadata(&source, message).await
            }

            Channel::CommandMetadata { operation } => {
                self.convert_command_metadata(&source, operation, message)
                    .await
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

            // Measurements, events, alarms and health statuses are converted by flows
            _ => Ok(vec![]),
        }
    }

    /// Registers the capability declared by an entity for an operation
    async fn convert_command_metadata(
        &mut self,
        source: &EntityTopicId,
        operation: &OperationType,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        // https://github.com/thin-edge/thin-edge.io/issues/2739
        if message.payload().is_empty() {
            warn!(topic = ?message.topic.name, "Ignoring command metadata clearing message: clearing device capabilities is not currently supported");
            return Ok(vec![]);
        }
        match operation {
            OperationType::Restart => self.register_restart_operation(source).await,
            OperationType::SoftwareList => {
                self.register_software_list_operation(source, message).await
            }
            OperationType::SoftwareUpdate => {
                self.register_software_update_operation(source, message)
                    .await
            }
            OperationType::LogUpload => self.convert_log_metadata(source, message).await,
            OperationType::ConfigSnapshot => {
                self.convert_config_snapshot_metadata(source, message).await
            }
            OperationType::ConfigUpdate => {
                self.convert_config_update_metadata(source, message).await
            }
            OperationType::FirmwareUpdate => self.register_firmware_update_operation(source).await,
            OperationType::DeviceProfile => self.register_device_profile_operation(source).await,
            OperationType::Custom(command_name) => {
                self.register_custom_operation(source, command_name).await
            }
            _ => Ok(vec![]),
        }
    }

    async fn register_restart_operation(
        &mut self,
        target: &EntityTopicId,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if !self.config.capabilities.device_restart {
            warn!("Received restart metadata, however, device restart feature is disabled");
            return Ok(vec![]);
        }

        Ok(self
            .register_operation(target, "c8y_Restart")
            .await
            .unwrap_or_else(|err| {
                error!("Failed to register the c8y_Restart operation for {target}: {err}");
                vec![]
            }))
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

        let Some(c8y_op_name) = self
            .supported_operations
            .operation_name_for_workflow(command_name)
        else {
            warn!("Failed to find the template file for `{command_name}`. Registration skipped");
            return Ok(vec![]);
        };
        Ok(self
            .register_operation(target, &c8y_op_name)
            .await
            .unwrap_or_else(|err| {
                error!("Failed to register the {c8y_op_name} operation for {target}: {err}");
                vec![]
            }))
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
        let topic = self.inventory_update_topic(target)?;

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
            Ok(messages) => messages,
            Err(err) => {
                error!("Failed to register the c8y_SoftwareUpdate operation for {target}: {err}");
                return Ok(vec![]);
            }
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

    // Messages received on Cumulocity topics

    async fn convert_c8y_message(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        self.commands.remove_expired();

        match &message.topic {
            topic
                if C8yDeviceControlTopic::accept(topic, &self.config.bridge_config.c8y_prefix) =>
            {
                self.convert_device_control_operations(message).await
            }
            topic
                if self
                    .supported_operations
                    .json_custom_operation_topics()?
                    .accept_topic(topic) =>
            {
                self.convert_json_custom_operation(message).await
            }
            topic
                if self
                    .supported_operations
                    .smartrest_custom_operation_topics()?
                    .accept_topic(topic) =>
            {
                self.run_smartrest_operations(message).await
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
            topic if self.http_event_topic.accept_topic(topic) => {
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
        }
    }

    /// Converts the operations received on `c8y/devicecontrol/notifications`, one
    /// per line
    async fn convert_device_control_operations(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let operation_payloads = message.payload_str()?.lines();

        let mut output = vec![];
        for operation_payload in operation_payloads {
            let operation = C8yOperation::from_json(operation_payload)?;
            let cmd_id = self.command_id.new_id_with_str(&operation.op_id);

            if self.commands.is_known(&cmd_id) {
                info!("{cmd_id} is already addressed");
                continue;
            }

            // wrap operation payload in a dummy MqttMessage wrapper because the code below
            // assumes 1 MQTT message = 1 operation
            // TODO: refactor to avoid this intermediate step and extra copies
            let operation_message = MqttMessage::new(&message.topic, operation_payload);

            let result = self
                .process_json_over_mqtt(&operation, cmd_id.clone(), &operation_message)
                .await;
            let result = self.operation_result_messages(
                result,
                &operation.external_source.external_id,
                Some(operation.op_id.clone()),
            );
            self.commands.started(cmd_id);
            output.extend(result);
        }

        Ok(output)
    }

    /// Converts a built-in operation into a thin-edge command, or hands a custom
    /// operation over
    async fn process_json_over_mqtt(
        &mut self,
        c8y_operation: &C8yOperation,
        cmd_id: CmdId,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let device_xid = c8y_operation.external_source.external_id.clone();
        let extras = &c8y_operation.extras;

        let operation = C8yDeviceControlOperation::from_json_object(extras)?;
        if let Some((operation_name, feature)) = self.disabled_feature_for(&operation) {
            warn!("Received a {operation_name} operation, however, {feature} feature is disabled");
            return Ok(vec![]);
        }

        let msgs = match operation {
            C8yDeviceControlOperation::Restart(_) => {
                self.convert_restart_request(device_xid, cmd_id)?
            }
            C8yDeviceControlOperation::SoftwareUpdate(request) => {
                self.convert_software_update_request(device_xid, cmd_id, request)
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
                    .process_json_custom_operation(c8y_operation, cmd_id, message)
                    .await;
            }
        };

        Ok(msgs)
    }

    fn convert_restart_request(
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

    async fn convert_software_update_request(
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

    async fn convert_json_custom_operation(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let operation = C8yOperation::from_json(message.payload.as_str()?)?;
        let cmd_id = self.command_id.new_id_with_str(&operation.op_id);

        if self.commands.is_known(&cmd_id) {
            info!("{cmd_id} is already addressed");
            return Ok(vec![]);
        }

        let result = self
            .process_json_custom_operation(&operation, cmd_id.clone(), message)
            .await;

        let output = self.operation_result_messages(
            result,
            &operation.external_source.external_id,
            Some(operation.op_id),
        );
        self.commands.started(cmd_id);

        Ok(output)
    }

    /// Converts a custom operation into a workflow command, or runs the command of
    /// its operation file
    async fn process_json_custom_operation(
        &self,
        c8y_operation: &C8yOperation,
        cmd_id: CmdId,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let operation_id = &c8y_operation.op_id;
        let device_xid = c8y_operation.external_source.external_id.clone();
        let handlers = self.supported_operations.operation_handlers(
            &device_xid,
            &message.topic.name,
            &self.config.bridge_config.c8y_prefix,
        );

        if handlers.is_empty() {
            info!("No matched custom operation handler is found for the subscribed custom operation topics. The operation '{operation_id}' (ID) is ignored.");
        }

        let Some((on_fragment, custom_handler)) = handlers
            .iter()
            .find(|(on_fragment, _)| c8y_operation.extras.contains_key(on_fragment))
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
                self.execute_custom_operation(custom_handler, message, operation_id)
                    .await?;
            }
        }

        // MQTT messages are sent during the operation execution
        Ok(vec![])
    }

    /// Passes the messages through, or reports the error to Cumulocity by setting
    /// the operation to executing then failed
    fn operation_result_messages(
        &self,
        result: Result<Vec<MqttMessage>, CumulocityMapperError>,
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
                // Without an operation ID, Cumulocity fails the oldest executing
                // operation of this type, so the operation is set to executing first
                vec![
                    MqttMessage::new(&topic, c8y_operation.executing()),
                    MqttMessage::new(&topic, c8y_operation.failed(&err.to_string())),
                ]
            }

            Ok(msgs) => msgs,
        }
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

    // Helpers

    /// Register on C8y an operation capability for a device.
    ///
    /// Additionally when the target is a child device, operation directory for the
    /// device will be loaded and operations not already registered will be
    /// registered.
    ///
    /// Returns a Set Supported Operations (114) message if among registered
    /// operations there were new operations that were not announced to the cloud.
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
            // for devices other than the main device and services, dynamic update of
            // supported operations via file events is disabled, so we have to additionally
            // load new operations from the c8y operations for that device
            EntityType::ChildDevice | EntityType::Service => self
                .supported_operations
                .load_all(device.external_id.as_ref(), &self.config.bridge_config)?,

            // for main devices new operation files are loaded dynamically as they are
            // created, so only register one operation we need
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

    pub fn load_and_create_supported_operations_message(
        &mut self,
        external_id: &str,
    ) -> Result<MqttMessage, ConversionError> {
        self.supported_operations
            .load_all(external_id, &self.config.bridge_config)?;
        Ok(self
            .supported_operations
            .create_supported_operations(external_id, &self.config.bridge_config.c8y_prefix)?)
    }

    /// The Cumulocity operation name and the feature it requires, if that feature
    /// is disabled
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

    /// The built-in `shell_execute` command is only mapped to Cumulocity when the
    /// `c8y.enable.shell_execute` feature is enabled
    fn is_disabled_command(&self, command_name: &str) -> bool {
        command_name == SHELL_EXECUTE_OPERATION && !self.config.capabilities.shell_execute
    }

    fn request_software_list(&mut self, target: &EntityTopicId) -> MqttMessage {
        let cmd_id = self.command_id.new_id();
        let request = SoftwareListCommand::new(target, cmd_id.clone());
        self.commands.software_list_requested(cmd_id, target);
        request.command_message(&self.mqtt_schema)
    }

    /// The SmartREST topic to publish on for the given entity: `c8y/s/us` for the
    /// main device, `c8y/s/us/<external id>` for any other
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
    /// The external id is generated by transforming the EntityTopicId by replacing
    /// the `/` characters with `:` and then adding the main device id as a prefix,
    /// to namespace all the entities under that device.
    ///
    /// # Examples
    /// - `device/main//` => `DEVICE_COMMON_NAME`
    /// - `device/child001//` => `DEVICE_COMMON_NAME:device:child001`
    /// - `device/child001/service/service001` =>
    ///   `DEVICE_COMMON_NAME:device:child001:service:service001`
    /// - `factory01/hallA/packaging/belt001` =>
    ///   `DEVICE_COMMON_NAME:factory01:hallA:packaging:belt001`
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

    /// Rejects IDs containing `/`, `+` or `#`, which would break the MQTT topics
    /// named after them
    pub fn validate_external_id(id: &str) -> Result<EntityExternalId, InvalidExternalIdError> {
        match id.chars().find(|c| FORBIDDEN_ID_CHARS.contains(c)) {
            Some(invalid_char) => Err(InvalidExternalIdError {
                external_id: id.into(),
                invalid_char,
            }),
            None => Ok(id.into()),
        }
    }

    /// Logs the error and wraps it in a message for the errors topic
    pub fn new_error_message(&self, error: impl std::error::Error) -> MqttMessage {
        error!("Mapping error: {}", error);
        MqttMessage::new(&self.errors_topic, error.to_string())
    }
}

/// The ways the converter can fail to start: an invalid configuration or
/// unreadable operation files
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

pub fn create_get_pending_operations_message(prefix: &TopicPrefix) -> MqttMessage {
    let topic = C8yTopic::upstream_topic(prefix);
    MqttMessage::new(&topic, request_pending_operations())
}

#[cfg(test)]
pub(crate) mod tests;
