//! Converting Cumulocity Smartrest operation messages into local thin-edge operation messages.
use crate::entity_cache::CloudEntityMetadata;
use crate::supported_operations::operation::Operation;
use c8y_api::json_c8y_deserializer::C8yDeviceProfile;
use c8y_api::json_c8y_deserializer::C8yDownloadConfigFile;
use c8y_api::json_c8y_deserializer::C8yFirmware;
use c8y_api::json_c8y_deserializer::C8yLogfileRequest;
use c8y_api::json_c8y_deserializer::C8yServiceCommand;
use c8y_api::json_c8y_deserializer::C8yUploadConfigFile;
use c8y_api::smartrest::message_ids::SET_SUPPORTED_CONFIGURATIONS;
use c8y_api::smartrest::message_ids::SET_SUPPORTED_LOGS;
use serde_json::json;
use serde_json::Value;
use std::sync::Arc;
use tedge_api::commands::CommandStatus;
use tedge_api::commands::ConfigMetadata;
use tedge_api::commands::ConfigSnapshotCmdPayload;
use tedge_api::commands::ConfigUpdateCmdPayload;
use tedge_api::commands::ControlCmd;
use tedge_api::commands::ControlCmdMetadata;
use tedge_api::commands::ControlCmdPayload;
use tedge_api::commands::FirmwareUpdateCmdPayload;
use tedge_api::commands::LogMetadata;
use tedge_api::commands::LogUploadCmdPayload;
use tedge_api::control::ActionName;
use tedge_api::control::ServiceName;
use tedge_api::control::ServiceType;
use tedge_api::control::DEFAULT_SERVICE_TYPE;
use tedge_api::device_profile::ConfigPayload;
use tedge_api::device_profile::DeviceProfileCmdPayload;
use tedge_api::entity::EntityExternalId;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::StateExcerpt;
use tedge_api::Jsonify;
use tedge_mqtt_ext::MqttMessage;
use tracing::error;
use tracing::info;
use tracing::warn;

use crate::converter::CumulocityConverter;
use crate::error::ConversionError;
use crate::error::CumulocityMapperError;

impl CumulocityConverter {
    /// Converts a config_snapshot metadata message to
    /// - supported operation "c8y_UploadConfigFile"
    /// - supported config types
    pub async fn convert_config_snapshot_metadata(
        &mut self,
        topic_id: &EntityTopicId,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if !self.config.capabilities.config_snapshot {
            warn!(
                "Received config_snapshot metadata, however, config_snapshot feature is disabled"
            );
        }
        self.convert_config_metadata(topic_id, message, "c8y_UploadConfigFile")
            .await
    }

    /// Converts a config_update metadata message to
    /// - supported operation "c8y_DownloadConfigFile"
    /// - supported config types
    pub async fn convert_config_update_metadata(
        &mut self,
        topic_id: &EntityTopicId,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if !self.config.capabilities.config_update {
            warn!("Received config_update metadata, however, config_update feature is disabled");
            return Ok(vec![]);
        }
        self.convert_config_metadata(topic_id, message, "c8y_DownloadConfigFile")
            .await
    }

    /// Convert c8y_UploadConfigFile JSON over MQTT operation to ThinEdge config_snapshot command
    pub fn convert_config_snapshot_request(
        &self,
        device_xid: String,
        cmd_id: String,
        config_upload_request: C8yUploadConfigFile,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let target = self
            .entity_cache
            .try_get_by_external_id(&device_xid.into())?;

        let channel = Channel::Command {
            operation: OperationType::ConfigSnapshot,
            cmd_id: cmd_id.clone(),
        };
        let topic = self.mqtt_schema.topic_for(target.topic_id(), &channel);

        // Replace '/' with ':' to avoid creating unexpected directories in file transfer repo
        let tedge_url = self.config.file_transfer_urls.for_path(&format!(
            "{}/config_snapshot/{}-{}",
            target.external_id.as_ref(),
            config_upload_request.config_type.replace('/', ":"),
            cmd_id
        ));

        let request = ConfigSnapshotCmdPayload {
            status: CommandStatus::Init,
            tedge_url: Some(tedge_url),
            config_type: config_upload_request.config_type,
            path: None,
            log_path: None,
        };

        // Command messages must be retained
        let payload = request.to_json();
        info!(target: "C8Y", topic = %topic.name, payload = %payload, "Forwarding c8y operation to device");
        Ok(vec![MqttMessage::new(&topic, payload).with_retain()])
    }

    /// Convert c8y_LogfileRequest operation to a ThinEdge log_upload command
    pub fn convert_log_upload_request(
        &self,
        device_xid: String,
        cmd_id: String,
        log_request: C8yLogfileRequest,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let target = self
            .entity_cache
            .try_get_by_external_id(&device_xid.into())?;

        let channel = Channel::Command {
            operation: OperationType::LogUpload,
            cmd_id: cmd_id.clone(),
        };
        let topic = self.mqtt_schema.topic_for(target.topic_id(), &channel);

        let tedge_url = self.config.file_transfer_urls.for_path(&format!(
            "{}/log_upload/{}-{}",
            target.external_id.as_ref(),
            log_request.log_file,
            cmd_id
        ));

        let request = LogUploadCmdPayload {
            status: CommandStatus::Init,
            tedge_url,
            log_type: log_request.log_file,
            date_from: log_request.date_from,
            date_to: log_request.date_to,
            search_text: Some(log_request.search_text).filter(|s| !s.is_empty()),
            lines: log_request.maximum_lines,
            log_path: None,
        };

        // Command messages must be retained
        let payload = request.to_json();
        info!(target: "C8Y", topic = %topic.name, payload = %payload, "Forwarding c8y operation to device");
        Ok(vec![MqttMessage::new(&topic, payload).with_retain()])
    }

    /// Converts a log_upload metadata message to
    /// - supported operation "c8y_LogfileRequest"
    /// - supported log types
    pub async fn convert_log_metadata(
        &mut self,
        topic_id: &EntityTopicId,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if !self.config.capabilities.log_upload {
            warn!("Received log_upload metadata, however, log_upload feature is disabled");
            return Ok(vec![]);
        }

        let mut messages = match self
            .register_operation(topic_id, "c8y_LogfileRequest")
            .await
        {
            Err(err) => {
                error!(
                    "Failed to register `c8y_LogfileRequest` operation for {topic_id} due to: {err}"
                );
                return Ok(vec![]);
            }
            Ok(messages) => messages,
        };

        // To SmartREST supported log types
        let metadata = LogMetadata::from_json(message.payload_str()?)?;
        let mut types = metadata.types;
        types.sort();
        let supported_log_types = types.join(",");
        let payload = format!("{SET_SUPPORTED_LOGS},{supported_log_types}");
        let c8y_topic = self.smartrest_publish_topic_for_entity(topic_id)?;
        messages.push(MqttMessage::new(&c8y_topic, payload));

        Ok(messages)
    }

    /// Convert c8y_Firmware JSON over MQTT operation to ThinEdge firmware_update command.
    pub fn convert_firmware_update_request(
        &self,
        device_xid: String,
        cmd_id: String,
        firmware_request: C8yFirmware,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let entity_xid: EntityExternalId = device_xid.into();

        let target = self.entity_cache.try_get_by_external_id(&entity_xid)?;

        let channel = Channel::Command {
            operation: OperationType::FirmwareUpdate,
            cmd_id,
        };
        let topic = self.mqtt_schema.topic_for(target.topic_id(), &channel);

        let tedge_url = if let Ok(c8y_url) = self.http_proxy.local_proxy_url(&firmware_request.url)
        {
            c8y_url.to_string()
        } else {
            firmware_request.url.clone()
        };

        let request = FirmwareUpdateCmdPayload {
            status: CommandStatus::Init,
            tedge_url: Some(tedge_url),
            remote_url: firmware_request.url,
            name: firmware_request.name,
            version: firmware_request.version,
            log_path: None,
        };

        // Command messages must be retained
        let payload = request.to_json();
        info!(target: "C8Y", topic = %topic.name, payload = %payload, "Forwarding c8y operation to device");
        Ok(vec![MqttMessage::new(&topic, payload).with_retain()])
    }

    pub async fn register_firmware_update_operation(
        &mut self,
        topic_id: &EntityTopicId,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if !self.config.capabilities.firmware_update {
            warn!(
                "Received firmware_update metadata, however, firmware_update feature is disabled"
            );
            return Ok(vec![]);
        }

        match self.register_operation(topic_id, "c8y_Firmware").await {
            Err(err) => {
                error!("Failed to register `c8y_Firmware` operation for {topic_id} due to: {err}");
                Ok(vec![])
            }
            Ok(messages) => Ok(messages),
        }
    }

    /// Upon receiving a SmartREST c8y_DownloadConfigFile request, convert it to a message on the
    /// command channel.
    pub async fn convert_config_update_request(
        &self,
        device_xid: String,
        cmd_id: String,
        config_download_request: C8yDownloadConfigFile,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let entity_xid: EntityExternalId = device_xid.into();
        let target = self.entity_cache.try_get_by_external_id(&entity_xid)?;

        let message = self.create_config_update_cmd(
            cmd_id.into(),
            &config_download_request,
            target.topic_id(),
        );
        Ok(message)
    }

    fn create_config_update_cmd(
        &self,
        cmd_id: Arc<str>,
        config_download_request: &C8yDownloadConfigFile,
        target: &EntityTopicId,
    ) -> Vec<MqttMessage> {
        let channel = Channel::Command {
            operation: OperationType::ConfigUpdate,
            cmd_id: cmd_id.to_string(),
        };
        let topic = self.mqtt_schema.topic_for(target, &channel);

        let remote_url = self
            .http_proxy
            .local_proxy_url(&config_download_request.url)
            .map(|url| url.to_string())
            .unwrap_or(config_download_request.url.to_string());

        let request = ConfigUpdateCmdPayload {
            status: CommandStatus::Init,
            tedge_url: None,
            remote_url,
            server_url: config_download_request.url.clone(),
            config_type: config_download_request.config_type.clone(),
            path: None,
            log_path: None,
        };

        // Command messages must be retained
        let payload = request.to_json();
        info!(target: "C8Y", topic = %topic.name, payload = %payload, "Forwarding c8y operation to device");
        vec![MqttMessage::new(&topic, payload).with_retain()]
    }

    async fn convert_config_metadata(
        &mut self,
        topic_id: &EntityTopicId,
        message: &MqttMessage,
        c8y_op_name: &str,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let metadata = ConfigMetadata::from_json(message.payload_str()?)?;

        let mut messages = match self.register_operation(topic_id, c8y_op_name).await {
            Err(err) => {
                error!("Failed to register {c8y_op_name} operation for {topic_id} due to: {err}");
                return Ok(vec![]);
            }
            Ok(messages) => messages,
        };

        // To SmartREST supported config types
        let mut types = metadata.types;
        types.sort();
        let supported_config_types = types.join(",");
        let payload = format!("{SET_SUPPORTED_CONFIGURATIONS},{supported_config_types}");
        let sm_topic = self.smartrest_publish_topic_for_entity(topic_id)?;
        messages.push(MqttMessage::new(&sm_topic, payload));

        Ok(messages)
    }

    /// Convert c8y_DeviceProfile JSON over MQTT operation to ThinEdge device_profile command.
    pub fn convert_device_profile_request(
        &self,
        device_xid: String,
        cmd_id: String,
        device_profile_request: C8yDeviceProfile,
        profile_name: String,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let entity_xid: EntityExternalId = device_xid.into();

        let target = self.entity_cache.try_get_by_external_id(&entity_xid)?;

        let channel = Channel::Command {
            operation: OperationType::DeviceProfile,
            cmd_id,
        };
        let topic = self.mqtt_schema.topic_for(target.topic_id(), &channel);

        let mut request = DeviceProfileCmdPayload {
            status: CommandStatus::Init,
            name: profile_name,
            operations: Vec::new(),
        };

        if let Some(mut firmware) = device_profile_request.firmware {
            if let Ok(cumulocity_url) = self.http_proxy.local_proxy_url(&firmware.url) {
                firmware.url = cumulocity_url.into();
            }
            request.add_firmware(firmware.into());
        }

        // Process configuration before installing software as it is fulfills more out-of-the-box scenarios than the other way around
        //
        // Advantages
        // * Repositories can be configured (with credentials or proxy settings) before applications/containers are installed via software
        // * Configuration can be applied before the application/container starts the first time (also removes need for restarting the software)
        //
        // Disadvantages
        // * Short period of time where potentially incompatible configuration is applied before the new application version has been installed
        // * Potential that application could overwrite config (generally only occurs if app builders haven't defined the linux package definition correctly)
        //
        // See https://github.com/thin-edge/thin-edge.io/issues/3684 for more details
        for config in device_profile_request.configuration {
            let remote_url = if let Ok(c8y_url) = self.http_proxy.local_proxy_url(&config.url) {
                c8y_url.to_string()
            } else {
                config.url.clone()
            };

            let config = ConfigPayload {
                name: config.name,
                config_type: config.config_type,
                remote_url: Some(remote_url),
                server_url: Some(config.url),
            };

            request.add_config(config);
        }

        if let Some(mut software) = device_profile_request.software {
            software.lists.iter_mut().for_each(|module| {
                if let Some(url) = &mut module.url {
                    if let Ok(cumulocity_url) = self.http_proxy.local_proxy_url(url) {
                        *url = cumulocity_url.into();
                    }
                }
            });
            request.add_software(software.try_into()?);
        }

        // Command messages must be retained
        let payload = request.to_json();
        info!(target: "C8Y", topic = %topic.name, payload = %payload, "Forwarding c8y operation to device");
        Ok(vec![MqttMessage::new(&topic, payload).with_retain()])
    }

    /// Converts a device_profile metadata message to supported operation "c8y_DeviceProfile"
    pub async fn register_device_profile_operation(
        &mut self,
        topic_id: &EntityTopicId,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if !self.config.capabilities.device_profile {
            warn!("Received device_profile metadata, however, device_profile feature is disabled");
            return Ok(vec![]);
        }

        match self.register_operation(topic_id, "c8y_DeviceProfile").await {
            Err(err) => {
                error!(
                    "Failed to register `device_profile` operation for {topic_id} due to: {err}"
                );
                Ok(vec![])
            }
            Ok(messages) => Ok(messages),
        }
    }

    pub fn convert_custom_operation_request(
        &self,
        device_xid: String,
        cmd_id: String,
        command_name: String,
        custom_handler: &Operation,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let entity_xid: EntityExternalId = device_xid.into();

        let target = self.entity_cache.try_get_by_external_id(&entity_xid)?;

        let channel = Channel::Command {
            operation: OperationType::Custom(command_name),
            cmd_id,
        };

        let topic = self.mqtt_schema.topic_for(target.topic_id(), &channel);

        let state = GenericCommandState::from_command_message(message).map_err(|e| {
            CumulocityMapperError::JsonCustomOperationHandlerError {
                operation: custom_handler.name.clone(),
                err_msg: format!("Invalid JSON message, {e}. Message: {message:?}"),
            }
        })?;

        let payload: Value = if let Some(workflow_input) = custom_handler.workflow_input() {
            let excerpt = StateExcerpt::from(workflow_input.clone());
            match excerpt.extract_value_from(&state) {
                Value::Object(obj) => Value::Object(obj),
                _ => {
                    error!(
                        "Operation file {} contains invalid value for `exec.workflow.input`. Skipping",
                        custom_handler.name
                    );
                    return Ok(vec![]);
                }
            }
        } else {
            serde_json::Value::Object(serde_json::Map::new())
        };

        let mapper_id = self.command_id.prefix();
        let inject_object = json!({
            mapper_id: {
                "on_fragment": custom_handler.on_fragment(),
                "output": custom_handler.workflow_output(),
            }
        });

        let request = GenericCommandState::new(topic, CommandStatus::Init.to_string(), payload)
            .update_with_json(inject_object);

        info!(target: "C8Y", topic = %request.topic, payload = %request.payload, "Forwarding c8y operation to device");
        Ok(vec![request.into_message()])
    }

    pub(crate) async fn convert_control_metadata(
        &mut self,
        service: &EntityTopicId,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if !self.config.capabilities.service_control {
            warn!("Received control metadata, however, service_control feature is disabled");
            return Ok(vec![]);
        }

        let mut messages = vec![];

        let actions = if message.payload_bytes().is_empty() {
            vec![]
        } else {
            let capability = match ControlCmdMetadata::from_slice(message.payload_bytes()) {
                Ok(capability) => capability,
                Err(err) => {
                    warn!(
                        topic = %message.topic.name,
                        "Ignoring the control capability of a service: {err}"
                    );
                    return Ok(vec![]);
                }
            };

            // The operation directory of a service is not watched,
            // so the operation is registered here
            match self.register_operation(service, "c8y_ServiceCommand").await {
                Ok(mut registration) => messages.append(&mut registration),
                Err(err) => error!(
                    "Failed to register the `c8y_ServiceCommand` operation of {service}: {err}"
                ),
            }

            capability
                .actions
                .into_iter()
                .filter_map(|action| match action.parse::<ActionName>() {
                    Ok(action) => Some(action),
                    Err(err) => {
                        warn!(
                            topic = %message.topic.name,
                            "Not declaring the action of a service to Cumulocity: {err}"
                        );
                        None
                    }
                })
                .collect()
        };

        messages.push(self.inventory_update_message(
            service,
            json!({ "c8y_SupportedServiceCommands": actions }),
        )?);
        self.service_actions.insert(service.clone(), actions);

        Ok(messages)
    }

    pub(crate) fn convert_control_request(
        &self,
        device_xid: String,
        cmd_id: String,
        request: C8yServiceCommand,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let target = self
            .entity_cache
            .try_get_by_external_id(&device_xid.into())
            .map_err(|err| CumulocityMapperError::ServiceCommandRejected {
                reason: err.to_string(),
            })?;

        let (action, service_name, service_type) = self
            .validate_service_command(target, &request)
            .map_err(|reason| CumulocityMapperError::ServiceCommandRejected { reason })?;

        let command = ControlCmd {
            target: target.topic_id().clone(),
            cmd_id,
            payload: ControlCmdPayload {
                status: CommandStatus::Init,
                action: action.to_string(),
                service_name: service_name.to_string(),
                service_type: service_type.to_string(),
            },
        };

        Ok(vec![command.command_message(&self.mqtt_schema)])
    }

    /// Checks a `c8y_ServiceCommand` request and resolves the service name and type to run it with.
    ///
    /// Returns the reason to fail the operation with when the request cannot be run.
    fn validate_service_command(
        &self,
        target: &CloudEntityMetadata,
        request: &C8yServiceCommand,
    ) -> Result<(ActionName, ServiceName, ServiceType), String> {
        let xid = target.external_id.as_ref();
        let command = &request.command;

        let action = command
            .parse::<ActionName>()
            .map_err(|err| format!("{xid} cannot run the command '{command}': {err}"))?;
        if !self
            .service_actions
            .get(target.topic_id())
            .is_some_and(|actions| actions.contains(&action))
        {
            return Err(format!("{xid} has not declared the '{action}' action"));
        }

        let service_name = Some(request.service_name.as_str())
            .filter(|name| !name.is_empty())
            .or_else(|| target.registered_name())
            .ok_or_else(|| {
                format!("{xid} cannot run the '{action}' action: neither the operation nor the registration of the service gives a service name")
            })?;
        // `tedge service` validates the name too,
        // but only a check here reports the reason on the operation
        let service_name = service_name
            .parse::<ServiceName>()
            .map_err(|err| format!("{xid} cannot run the '{action}' action: {err}"))?;

        let service_type = Some(request.service_type.as_str())
            .filter(|ty| !ty.is_empty())
            .or_else(|| target.registered_type())
            .unwrap_or(DEFAULT_SERVICE_TYPE)
            .parse::<ServiceType>()
            .map_err(|err| format!("{xid} cannot run the '{action}' action: {err}"))?;

        Ok((action, service_name, service_type))
    }
}
