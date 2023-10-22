use crate::converter::CumulocityConverter;
use crate::error::ConversionError;
use crate::error::CumulocityMapperError;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::smartrest::smartrest_deserializer::SmartRestLogRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRestSerializer;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToExecuting;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToFailed;
use std::collections::HashMap;
use tedge_actors::Sender;
use tedge_api::entity_store::EntityType;
use tedge_api::messages::CommandStatus;
use tedge_api::messages::LogMetadata;
use tedge_api::messages::LogUploadCmdPayload;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter::Command;
use tedge_api::mqtt_topics::ChannelFilter::CommandMetadata;
use tedge_api::mqtt_topics::EntityFilter::AnyEntity;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::Jsonify;
use tedge_mqtt_ext::Message;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::TopicFilter;
use tedge_uploader_ext::UploadRequest;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::create_file_with_defaults;
use time::OffsetDateTime;
use tracing::log::warn;

pub fn log_upload_topic_filter(mqtt_schema: &MqttSchema) -> TopicFilter {
    [
        mqtt_schema.topics(AnyEntity, Command(OperationType::LogUpload)),
        mqtt_schema.topics(AnyEntity, CommandMetadata(OperationType::LogUpload)),
    ]
    .into_iter()
    .collect()
}

impl CumulocityConverter {
    /// Convert a SmartREST logfile request to a Thin Edge log_upload command
    pub fn convert_log_upload_request(
        &self,
        smartrest: &str,
    ) -> Result<Vec<Message>, CumulocityMapperError> {
        if !self.config.capabilities.log_upload {
            warn!(
                "Received a c8y_LogfileRequest operation, however, log_upload feature is disabled"
            );
            return Ok(vec![]);
        }

        let log_request = SmartRestLogRequest::from_smartrest(smartrest)?;
        let device_external_id = log_request.device.into();
        let target = self
            .entity_store
            .try_get_by_external_id(&device_external_id)?;

        let cmd_id = self.command_id.new_id();
        let channel = Channel::Command {
            operation: OperationType::LogUpload,
            cmd_id: cmd_id.clone(),
        };
        let topic = self.mqtt_schema.topic_for(&target.topic_id, &channel);

        let tedge_url = format!(
            "http://{}/tedge/file-transfer/{}/log_upload/{}-{}",
            &self.config.tedge_http_host,
            target.external_id.as_ref(),
            log_request.log_type,
            cmd_id
        );

        let request = LogUploadCmdPayload {
            status: CommandStatus::Init,
            tedge_url,
            log_type: log_request.log_type,
            date_from: log_request.date_from,
            date_to: log_request.date_to,
            search_text: log_request.search_text,
            lines: log_request.lines,
        };

        // Command messages must be retained
        Ok(vec![Message::new(&topic, request.to_json()).with_retain()])
    }

    /// Address a received log_upload command. If its status is
    /// - "executing", it converts the message to SmartREST "Executing".
    /// - "successful", it creates an event in c8y, then creates an UploadRequest for the uploader actor.
    /// - "failed", it converts the message to SmartREST "Failed".
    pub async fn handle_log_upload_state_change(
        &mut self,
        topic_id: &EntityTopicId,
        cmd_id: &str,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        if !self.config.capabilities.log_upload {
            warn!("Received a log_upload command, however, log_upload feature is disabled");
            return Ok(vec![]);
        }

        let target = self.entity_store.try_get(topic_id)?;
        let smartrest_topic = self.smartrest_publish_topic_for_entity(topic_id)?;
        let payload = message.payload_str()?;
        let response = &LogUploadCmdPayload::from_json(payload)?;

        let messages = match response.status {
            CommandStatus::Executing => {
                let smartrest_operation_status = SmartRestSetOperationToExecuting::new(
                    CumulocitySupportedOperations::C8yLogFileRequest,
                )
                .to_smartrest()?;
                vec![Message::new(&smartrest_topic, smartrest_operation_status)]
            }
            CommandStatus::Successful => {
                // Create an event in c8y
                let c8y_event = C8yCreateEvent::new_from_entity(
                    target,
                    &response.log_type,
                    OffsetDateTime::now_utc(),
                    &response.log_type,
                    HashMap::new(),
                );
                let event_response_id = self.http_proxy.send_event(c8y_event).await?;

                // Send a request to the Uploader to upload the file asynchronously.
                let uploaded_file_path = self
                    .config
                    .data_dir
                    .file_transfer_dir()
                    .join(target.external_id.as_ref())
                    .join("log_upload")
                    .join(format!("{}-{}", response.log_type, cmd_id));

                let binary_upload_event_url = self
                    .c8y_endpoint
                    .try_get_url_for_event_binary_upload(&event_response_id);

                let upload_request = UploadRequest::new(
                    self.auth_proxy.proxy_url(binary_upload_event_url).as_str(),
                    &uploaded_file_path,
                );
                self.uploader_sender
                    .send((cmd_id.into(), upload_request))
                    .await
                    .map_err(CumulocityMapperError::ChannelError)?;

                self.pending_upload_operations.insert(
                    cmd_id.into(),
                    (
                        smartrest_topic,
                        message.topic.clone(),
                        CumulocitySupportedOperations::C8yLogFileRequest,
                    ),
                );

                vec![] // No mqtt message can be published in this state
            }
            CommandStatus::Failed { ref reason } => {
                let smartrest_operation_status = SmartRestSetOperationToFailed::new(
                    CumulocitySupportedOperations::C8yLogFileRequest,
                    reason.clone(),
                )
                .to_smartrest()?;
                let c8y_notification = Message::new(&smartrest_topic, smartrest_operation_status);
                let clean_operation = Message::new(&message.topic, "")
                    .with_retain()
                    .with_qos(QoS::AtLeastOnce);
                vec![c8y_notification, clean_operation]
            }
            _ => {
                vec![] // Do nothing as other components might handle those states
            }
        };

        Ok(messages)
    }

    /// Converts a log_upload metadata message to
    /// - supported operation "c8y_LogfileRequest"
    /// - supported log types
    pub fn convert_log_metadata(
        &mut self,
        topic_id: &EntityTopicId,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        if !self.config.capabilities.log_upload {
            warn!("Received log_upload metadata, however, log_upload feature is disabled");
            return Ok(vec![]);
        }

        let metadata = LogMetadata::from_json(message.payload_str()?)?;

        // get the device metadata from its id
        let target = self.entity_store.try_get(topic_id)?;

        // Create a c8y_LogfileRequest operation file
        let dir_path = match target.r#type {
            EntityType::MainDevice => self.ops_dir.clone(),
            EntityType::ChildDevice => {
                let child_dir_name = target.external_id.as_ref();
                self.ops_dir.clone().join(child_dir_name)
            }
            EntityType::Service => {
                // No support for service log management
                return Ok(vec![]);
            }
        };
        create_directory_with_defaults(&dir_path)?;
        create_file_with_defaults(dir_path.join("c8y_LogfileRequest"), None)?;

        // To SmartREST supported log types
        let mut types = metadata.types;
        types.sort();
        let supported_log_types = types.join(",");
        let payload = format!("118,{supported_log_types}");

        let c8y_topic = self.smartrest_publish_topic_for_entity(topic_id)?;
        Ok(vec![MqttMessage::new(&c8y_topic, payload)])
    }
}
