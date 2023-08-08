use crate::converter::CumulocityConverter;
use crate::error::ConversionError;
use crate::error::CumulocityMapperError;
use c8y_api::smartrest::smartrest_deserializer::SmartRestLogRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRestSerializer;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToExecuting;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToFailed;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToSuccessful;
use c8y_api::smartrest::topic::C8yTopic;
use nanoid::nanoid;
use tedge_api::cmd_topic::get_target_ids_from_cmd_topic;
use tedge_api::cmd_topic::CmdPublishTopic;
use tedge_api::cmd_topic::CmdSubscribeTopic;
use tedge_api::cmd_topic::DeviceKind;
use tedge_api::cmd_topic::Target;
use tedge_api::messages::CommandStatus;
use tedge_api::messages::LogMetadata;
use tedge_api::messages::LogUploadCmdPayload;
use tedge_api::Jsonify;
use tedge_mqtt_ext::Message;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::TopicFilter;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::create_file_with_defaults;

pub fn log_upload_topic_filter(topic_root: &str) -> TopicFilter {
    vec![
        CmdSubscribeTopic::LogUpload.metadata(topic_root).as_str(),
        CmdSubscribeTopic::LogUpload.with_id(topic_root).as_str(),
    ]
    .try_into()
    .unwrap()
}

impl CumulocityConverter {
    /// Convert a SmartREST logfile request to a Thin Edge log_upload command
    pub fn convert_log_upload_request(
        &self,
        smartrest: &str,
    ) -> Result<Vec<Message>, CumulocityMapperError> {
        let log_request = SmartRestLogRequest::from_smartrest(smartrest)?;
        let device_id = if log_request.device.eq(&self.device_name) {
            "main".into()
        } else {
            log_request.device
        };

        let cmd_id = nanoid!();
        let topic = CmdPublishTopic::LogUpload(Target::new(device_id.clone(), cmd_id.clone()))
            .to_topic("te");

        let tedge_url = format!(
            "http://{}/tedge/file-transfer/{}/log_upload/{}-{}",
            &self.config.tedge_http_host, device_id, log_request.log_type, cmd_id
        );

        let request = LogUploadCmdPayload {
            status: CommandStatus::Init,
            tedge_url,
            log_type: log_request.log_type,
            date_from: log_request.date_from,
            date_to: log_request.date_to,
            search_text: log_request.search_text,
            lines: log_request.lines,
            reason: None,
        };

        // Command messages must be retained
        Ok(vec![Message::new(&topic, request.to_json()?).with_retain()])
    }

    /// Address a received log_upload command. If its status is
    /// - "executing", it converts the message to SmartREST "Executing".
    /// - "successful", it uploads a log file to c8y and converts the message to SmartREST "Successful".
    /// - "failed", it converts the message to SmartREST "Failed".
    pub async fn handle_log_upload_state_change(
        &mut self,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        let (device_kind, cmd_id) =
            match get_target_ids_from_cmd_topic(&message.topic, &self.config.topic_root) {
                (Some(device_kind), Some(cmd_id)) => (device_kind, cmd_id),
                _ => {
                    return Err(ConversionError::UnsupportedTopic(
                        message.topic.name.clone(),
                    ))
                }
            };

        let external_id = device_kind.name_with_default(&self.device_name);

        let c8y_topic: C8yTopic = device_kind.clone().into();
        let smartrest_topic = c8y_topic.to_topic()?;

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
                let uploaded_file_path = self
                    .config
                    .file_transfer_dir
                    .join(device_kind.name())
                    .join("log_upload")
                    .join(format!("{}-{}", response.log_type, cmd_id));
                let result = self
                    .http_proxy
                    .upload_file(
                        uploaded_file_path.as_std_path(),
                        &response.log_type,
                        external_id,
                    )
                    .await; // We need to get rid of this await, otherwise it blocks

                let smartrest_operation_status = match result {
                    Ok(url) => SmartRestSetOperationToSuccessful::new(
                        CumulocitySupportedOperations::C8yLogFileRequest,
                    )
                    .with_response_parameter(&url)
                    .to_smartrest()?,
                    Err(err) => SmartRestSetOperationToFailed::new(
                        CumulocitySupportedOperations::C8yLogFileRequest,
                        format!("Upload failed with {}", err),
                    )
                    .to_smartrest()?,
                };

                let c8y_notification = Message::new(&smartrest_topic, smartrest_operation_status);
                let clean_operation = Message::new(&message.topic, "")
                    .with_retain()
                    .with_qos(QoS::AtLeastOnce);
                vec![c8y_notification, clean_operation]
            }
            CommandStatus::Failed => {
                let smartrest_operation_status = SmartRestSetOperationToFailed::new(
                    CumulocitySupportedOperations::C8yLogFileRequest,
                    response.reason.clone().unwrap_or_default(),
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
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        let metadata = LogMetadata::from_json(message.payload_str()?)?;

        let device_kind =
            match get_target_ids_from_cmd_topic(&message.topic, &self.config.topic_root).0 {
                Some(device_id) => device_id,
                _ => {
                    return Err(ConversionError::UnsupportedTopic(
                        message.topic.name.clone(),
                    ))
                }
            };

        // Create a c8y_LogfileRequest operation file
        let dir_path = match device_kind {
            DeviceKind::Main => self.ops_dir.clone(),
            DeviceKind::Child(ref id) => self.ops_dir.join(id),
        };
        create_directory_with_defaults(&dir_path)?;
        create_file_with_defaults(dir_path.join("c8y_LogfileRequest"), None)?;

        // To SmartREST supported log types
        let mut types = metadata.types;
        types.sort();
        let supported_log_types = types.join(",");
        let payload = format!("118,{supported_log_types}");

        let c8y_topic: C8yTopic = device_kind.into();
        Ok(vec![MqttMessage::new(&c8y_topic.to_topic()?, payload)])
    }
}
