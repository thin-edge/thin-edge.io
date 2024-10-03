//! Keeps track of c8y operations received from the cloud and responds to them.

use std::{collections::HashMap, sync::Arc};

use c8y_api::{
    http_proxy::C8yEndPoint,
    json_c8y_deserializer::{
        C8yDeviceControlOperation, C8yDeviceProfile, C8yDownloadConfigFile, C8yFirmware,
        C8yLogfileRequest, C8yOperation, C8yRestart, C8ySoftwareUpdate, C8yUploadConfigFile,
    },
};
use c8y_auth_proxy::url::ProxyUrlGenerator;
use tedge_api::{
    commands::{
        ConfigSnapshotCmdPayload, ConfigUpdateCmdPayload, FirmwareUpdateCmdPayload,
        LogUploadCmdPayload,
    },
    device_profile::DeviceProfileCmdPayload,
    entity_store::EntityMetadata,
    mqtt_topics::{Channel, EntityTopicId, IdGenerator, MqttSchema, OperationType},
    CommandStatus, DownloadInfo, Jsonify, RestartCommand,
};
use tedge_mqtt_ext::MqttMessage;
use tracing::{error, warn};

use crate::{
    error::{ConversionError, CumulocityMapperError},
    Capabilities,
};

type OperationId = Arc<str>;

#[derive(Debug, Clone)]
struct C8yOperations {
    active_c8y_operations: HashMap<OperationId, C8yOperation>,
    capabilities: Capabilities,
    xid_to_metadata: HashMap<Arc<str>, EntityMetadata>,
    mqtt_schema: MqttSchema,
    command_id: IdGenerator,
    c8y_endpoint: C8yEndPoint,
    auth_proxy: ProxyUrlGenerator,
    tedge_http_host: Arc<str>,
}

impl C8yOperations {
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            active_c8y_operations: HashMap::new(),
            capabilities,
            xid_to_metadata: HashMap::new(),
            mqtt_schema: MqttSchema::new(),
            command_id: IdGenerator::new("peniz"),
            c8y_endpoint: C8yEndPoint::new("peniz", "peniz", "peniz"),
            auth_proxy: ProxyUrlGenerator::new(
                "peniz".into(),
                2137,
                c8y_auth_proxy::url::Protocol::Http,
            ),
            tedge_http_host: Arc::from("peniz"),
        }
    }

    pub fn register(&mut self, operation: C8yOperation) {
        let entity_xid = &operation.external_source.external_id;

        let c8y_device_control_operation =
            C8yDeviceControlOperation::from_json_object(&operation.extras).unwrap();

        if !self.capabilities.is_enabled(&c8y_device_control_operation) {
            warn!("Received an operation which is disabled in configuration");
            return;
        }

        let entity_metadata = self.xid_to_metadata.get(entity_xid.as_str()).unwrap();
        let cmd_id = self.command_id.new_id();

        let msgs = match c8y_device_control_operation {
            C8yDeviceControlOperation::Restart(request) => self
                .forward_restart_request(entity_metadata, cmd_id)
                .unwrap(),
            C8yDeviceControlOperation::SoftwareUpdate(request) => self
                .forward_software_request(entity_metadata, cmd_id, request)
                .unwrap(),
            C8yDeviceControlOperation::LogfileRequest(request) => self
                .convert_log_upload_request(entity_metadata, cmd_id, request)
                .unwrap(),
            C8yDeviceControlOperation::UploadConfigFile(request) => self
                .convert_config_snapshot_request(entity_metadata, cmd_id, request)
                .unwrap(),
            C8yDeviceControlOperation::DownloadConfigFile(request) => self
                .convert_config_update_request(entity_metadata, cmd_id, request)
                .unwrap(),
            C8yDeviceControlOperation::Firmware(request) => self
                .convert_firmware_update_request(entity_metadata, cmd_id, request)
                .unwrap(),
            C8yDeviceControlOperation::DeviceProfile(request) => {
                if let Some(profile_name) = operation.extras.get("profileName") {
                    self.convert_device_profile_request(
                        entity_metadata,
                        cmd_id,
                        request,
                        serde_json::from_value(profile_name.clone()).unwrap(),
                    )
                    .unwrap()
                } else {
                    error!("Received a c8y_DeviceProfile without a profile name");
                    vec![]
                }
            }
            C8yDeviceControlOperation::Custom => {
                // Ignores custom and static template operations unsupported by thin-edge
                // However, these operations can be addressed by SmartREST that is published together with JSON over MQTT
                vec![]
            }
        };

        let id = Arc::from(operation.op_id.as_str());
        self.active_c8y_operations.insert(id, operation);

        // send local MQTT command
    }

    fn forward_restart_request(
        &self,
        entity: &EntityMetadata,
        cmd_id: String,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let command = RestartCommand::new(&entity.topic_id, cmd_id);
        let message = command.command_message(&self.mqtt_schema);
        Ok(vec![message])
    }

    fn forward_software_request(
        &self,
        entity: &EntityMetadata,
        cmd_id: String,
        software_update_request: C8ySoftwareUpdate,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let mut command =
            software_update_request.into_software_update_command(&entity.topic_id, cmd_id)?;

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

    /// Convert c8y_UploadConfigFile JSON over MQTT operation to ThinEdge config_snapshot command
    pub fn convert_config_snapshot_request(
        &self,
        entity: &EntityMetadata,
        cmd_id: String,
        config_upload_request: C8yUploadConfigFile,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let channel = Channel::Command {
            operation: OperationType::ConfigSnapshot,
            cmd_id: cmd_id.clone(),
        };
        let topic = self.mqtt_schema.topic_for(&entity.topic_id, &channel);

        // Replace '/' with ':' to avoid creating unexpected directories in file transfer repo
        let tedge_url = format!(
            "http://{}/tedge/file-transfer/{}/config_snapshot/{}-{}",
            &self.tedge_http_host,
            entity.external_id.as_ref(),
            config_upload_request.config_type.replace('/', ":"),
            cmd_id
        );

        let request = ConfigSnapshotCmdPayload {
            status: CommandStatus::Init,
            tedge_url: Some(tedge_url),
            config_type: config_upload_request.config_type,
            path: None,
            log_path: None,
        };

        // Command messages must be retained
        Ok(vec![
            MqttMessage::new(&topic, request.to_json()).with_retain()
        ])
    }

    /// Convert c8y_LogfileRequest operation to a ThinEdge log_upload command
    pub fn convert_log_upload_request(
        &self,
        entity: &EntityMetadata,
        cmd_id: String,
        log_request: C8yLogfileRequest,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let channel = Channel::Command {
            operation: OperationType::LogUpload,
            cmd_id: cmd_id.clone(),
        };
        let topic = self.mqtt_schema.topic_for(&entity.topic_id, &channel);

        let tedge_url = format!(
            "http://{}/tedge/file-transfer/{}/log_upload/{}-{}",
            &self.tedge_http_host,
            entity.external_id.as_ref(),
            log_request.log_file,
            cmd_id
        );

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
        Ok(vec![
            MqttMessage::new(&topic, request.to_json()).with_retain()
        ])
    }

    /// Convert c8y_Firmware JSON over MQTT operation to ThinEdge firmware_update command.
    pub fn convert_firmware_update_request(
        &self,
        entity: &EntityMetadata,
        cmd_id: String,
        firmware_request: C8yFirmware,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let channel = Channel::Command {
            operation: OperationType::FirmwareUpdate,
            cmd_id,
        };
        let topic = self.mqtt_schema.topic_for(&entity.topic_id, &channel);

        let tedge_url =
            if let Some(c8y_url) = self.c8y_endpoint.maybe_tenant_url(&firmware_request.url) {
                self.auth_proxy.proxy_url(c8y_url).to_string()
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
        Ok(vec![
            MqttMessage::new(&topic, request.to_json()).with_retain()
        ])
    }

    /// Upon receiving a SmartREST c8y_DownloadConfigFile request, convert it to a message on the
    /// command channel.
    pub fn convert_config_update_request(
        &self,
        entity: &EntityMetadata,
        cmd_id: String,
        config_download_request: C8yDownloadConfigFile,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let config_download_request: &C8yDownloadConfigFile = &config_download_request;
        let channel = Channel::Command {
            operation: OperationType::ConfigUpdate,
            cmd_id: cmd_id.to_string(),
        };
        let topic = self.mqtt_schema.topic_for(&entity.topic_id, &channel);

        let proxy_url = self
            .c8y_endpoint
            .maybe_tenant_url(&config_download_request.url)
            .map(|cumulocity_url| self.auth_proxy.proxy_url(cumulocity_url).into());

        let remote_url = proxy_url.unwrap_or(config_download_request.url.to_string());

        let request = ConfigUpdateCmdPayload {
            status: CommandStatus::Init,
            tedge_url: None,
            remote_url,
            config_type: config_download_request.config_type.clone(),
            path: None,
            log_path: None,
        };

        // Command messages must be retained
        let messages = vec![MqttMessage::new(&topic, request.to_json()).with_retain()];
        Ok(messages)
    }

    /// Convert c8y_DeviceProfile JSON over MQTT operation to ThinEdge device_profile command.
    pub fn convert_device_profile_request(
        &self,
        entity: &EntityMetadata,
        cmd_id: String,
        device_profile_request: C8yDeviceProfile,
        profile_name: String,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let channel = Channel::Command {
            operation: OperationType::DeviceProfile,
            cmd_id,
        };
        let topic = self.mqtt_schema.topic_for(&entity.topic_id, &channel);

        let mut request = DeviceProfileCmdPayload {
            status: CommandStatus::Init,
            name: profile_name,
            operations: Vec::new(),
        };

        if let Some(mut firmware) = device_profile_request.firmware {
            if let Some(cumulocity_url) = self.c8y_endpoint.maybe_tenant_url(&firmware.url) {
                firmware.url = self.auth_proxy.proxy_url(cumulocity_url).into();
            }
            request.add_firmware(firmware.into());
        }

        if let Some(mut software) = device_profile_request.software {
            software.lists.iter_mut().for_each(|module| {
                if let Some(url) = &mut module.url {
                    if let Some(cumulocity_url) = self.c8y_endpoint.maybe_tenant_url(url) {
                        *url = self.auth_proxy.proxy_url(cumulocity_url).into();
                    }
                }
            });
            request.add_software(software.try_into()?);
        }

        for mut config in device_profile_request.configuration {
            if let Some(cumulocity_url) = self.c8y_endpoint.maybe_tenant_url(&config.url) {
                config.url = self.auth_proxy.proxy_url(cumulocity_url).into();
            }
            request.add_config(config.into());
        }

        // Command messages must be retained
        Ok(vec![
            MqttMessage::new(&topic, request.to_json()).with_retain()
        ])
    }
}

/// Converts C8y operations into local MQTT commands
trait IntoCommand {
    fn into_command(
        self,
        topic_id: &EntityTopicId,
        cmd_id: String,
        mqtt_schema: &MqttSchema,
    ) -> Result<Vec<MqttMessage>, anyhow::Error>;
}

impl IntoCommand for C8yRestart {
    fn into_command(
        self,
        topic_id: &EntityTopicId,
        cmd_id: String,
        mqtt_schema: &MqttSchema,
    ) -> Result<Vec<MqttMessage>, anyhow::Error> {
        let command = RestartCommand::new(topic_id, cmd_id);
        let message = command.command_message(mqtt_schema);
        Ok(vec![message])
    }
}

// impl IntoCommand for C8ySoftwareUpdate {
//     fn into_command(
//         self,
//         topic_id: &EntityTopicId,
//         cmd_id: String,
//         mqtt_schema: &MqttSchema,
//     ) -> Result<Vec<MqttMessage>, anyhow::Error> {
//         let mut command = self.into_software_update_command(topic_id, cmd_id)?;

//         command.payload.update_list.iter_mut().for_each(|modules| {
//             modules.modules.iter_mut().for_each(|module| {
//                 if let Some(url) = &mut module.url {
//                     *url = if let Some(cumulocity_url) =
//                         self.c8y_endpoint.maybe_tenant_url(url.url())
//                     {
//                         DownloadInfo::new(self.auth_proxy.proxy_url(cumulocity_url).as_ref())
//                     } else {
//                         DownloadInfo::new(url.url())
//                     };
//                 }
//             });
//         });

//         let message = command.command_message(&self.mqtt_schema);
//         Ok(vec![message])
//     }
// }
