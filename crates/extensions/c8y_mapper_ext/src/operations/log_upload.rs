use super::FtsDownloadOperationData;
use super::FtsDownloadOperationType;
use crate::actor::CmdId;
use crate::converter::CumulocityConverter;
use crate::converter::UploadOperationData;
use crate::error::ConversionError;
use crate::error::CumulocityMapperError;
use anyhow::Context;
use c8y_api::smartrest::smartrest_deserializer::SmartRestLogRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_serializer::fail_operation;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_http_proxy::messages::CreateEvent;
use camino::Utf8PathBuf;
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
use tedge_downloader_ext::DownloadRequest;
use tedge_downloader_ext::DownloadResult;
use tedge_mqtt_ext::Message;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::TopicFilter;
use tedge_uploader_ext::ContentType;
use tedge_uploader_ext::UploadRequest;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::create_file_with_defaults;
use time::OffsetDateTime;
use tracing::debug;
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
    /// - "failed", it converts the message to SmartREST "Failed" with that event URL.
    pub async fn handle_log_upload_state_change(
        &mut self,
        topic_id: &EntityTopicId,
        cmd_id: &str,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        debug!("Handling log_upload command");

        if !self.config.capabilities.log_upload {
            warn!("Received a log_upload command, however, log_upload feature is disabled");
            return Ok(vec![]);
        }

        let target = self.entity_store.try_get(topic_id)?;
        let smartrest_topic = self.smartrest_publish_topic_for_entity(topic_id)?;
        let payload = message.payload_str()?;
        let response = &LogUploadCmdPayload::from_json(payload)?;

        let messages = match &response.status {
            CommandStatus::Executing => {
                let smartrest_operation_status =
                    set_operation_executing(CumulocitySupportedOperations::C8yLogFileRequest);
                vec![Message::new(&smartrest_topic, smartrest_operation_status)]
            }
            CommandStatus::Successful => {
                // Send a request to the Downloader to download the file asynchronously from FTS
                let log_filename = format!("{}-{}", response.log_type, cmd_id);

                let tedge_file_url = format!(
                    "http://{}/tedge/file-transfer/{external_id}/log_upload/{log_filename}",
                    &self.config.tedge_http_host,
                    external_id = target.external_id.as_ref()
                );

                let destination_dir = tempfile::tempdir_in(self.config.tmp_dir.as_std_path())
                    .context("Failed to create a temporary directory")?;
                let destination_path = destination_dir.path().join(log_filename);

                self.pending_fts_download_operations.insert(
                    cmd_id.into(),
                    FtsDownloadOperationData {
                        download_type: FtsDownloadOperationType::LogDownload,
                        url: tedge_file_url.clone(),
                        file_dir: destination_dir,

                        message: message.clone(),
                        entity_topic_id: topic_id.clone(),
                    },
                );

                let download_request = DownloadRequest::new(&tedge_file_url, &destination_path);

                self.downloader_sender
                    .send((cmd_id.into(), download_request))
                    .await
                    .map_err(CumulocityMapperError::ChannelError)?;

                // cont. in handle_fts_log_download

                vec![] // No mqtt message can be published in this state
            }
            CommandStatus::Failed { reason } => {
                let smartrest_operation_status =
                    fail_operation(CumulocitySupportedOperations::C8yLogFileRequest, reason);
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

    /// Resumes `log_upload` operation after required file was downloaded from
    /// the File Transfer Service.
    pub async fn handle_fts_log_download_result(
        &mut self,
        cmd_id: CmdId,
        download_result: DownloadResult,
        fts_download: FtsDownloadOperationData,
    ) -> Result<Vec<Message>, ConversionError> {
        let topic_id = fts_download.entity_topic_id;
        let target = self.entity_store.try_get(&topic_id)?;
        let smartrest_topic = self.smartrest_publish_topic_for_entity(&topic_id)?;
        let payload = fts_download.message.payload_str()?;
        let response = &LogUploadCmdPayload::from_json(payload)?;

        let download_response = match download_result {
            Err(err) => {
                let smartrest_error = fail_operation(
                    CumulocitySupportedOperations::C8yLogFileRequest,
                    &format!(
                        "tedge-mapper-c8y failed to download log from file transfer service: {err}",
                    ),
                );

                let c8y_notification = Message::new(&smartrest_topic, smartrest_error);
                let clean_operation = Message::new(&fts_download.message.topic, "")
                    .with_retain()
                    .with_qos(QoS::AtLeastOnce);
                return Ok(vec![c8y_notification, clean_operation]);
            }
            Ok(download) => download,
        };

        // Create an event in c8y
        let create_event = CreateEvent {
            event_type: response.log_type.clone(),
            time: OffsetDateTime::now_utc(),
            text: response.log_type.clone(),
            extras: HashMap::new(),
            device_id: target.external_id.as_ref().to_string(),
        };
        let event_response_id = self.http_proxy.send_event(create_event).await?;

        let binary_upload_event_url = self
            .c8y_endpoint
            .get_url_for_event_binary_upload_unchecked(&event_response_id);

        let upload_request = UploadRequest::new(
            self.auth_proxy
                .proxy_url(binary_upload_event_url.clone())
                .as_str(),
            &Utf8PathBuf::try_from(download_response.file_path).map_err(|e| e.into_io_error())?,
        )
        .with_content_type(ContentType::TextPlain);

        self.uploader_sender
            .send((cmd_id.clone(), upload_request))
            .await
            .map_err(CumulocityMapperError::ChannelError)?;

        self.pending_upload_operations.insert(
            cmd_id,
            UploadOperationData {
                file_dir: fts_download.file_dir,
                smartrest_topic,
                clear_cmd_topic: fts_download.message.topic,
                c8y_binary_url: binary_upload_event_url.to_string(),
                operation: CumulocitySupportedOperations::C8yLogFileRequest,
            },
        );

        Ok(vec![])
    }

    /// Converts a log_upload metadata message to
    /// - supported operation "c8y_LogfileRequest"
    /// - supported log types
    pub fn convert_log_metadata(
        &self,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;
    use c8y_api::smartrest::topic::C8yTopic;
    use serde_json::json;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::MessageReceiver;
    use tedge_downloader_ext::DownloadResponse;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::test_helpers::assert_received_includes_json;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;
    use tedge_uploader_ext::UploadResponse;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

    #[tokio::test]
    async fn mapper_converts_smartrest_logfile_req_to_log_upload_cmd_for_main_device() {
        let cfg_dir = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_LogfileRequest SmartREST request
        mqtt.send(MqttMessage::new(
            &C8yTopic::downstream_topic(),
            "522,test-device,logfileA,2013-06-22T17:03:14.123+02:00,2013-06-23T18:03:14.123+02:00,ERROR,1000",
        ))
            .await
            .expect("Send failed");

        let (topic, received_json) = mqtt
            .recv()
            .await
            .map(|msg| {
                (
                    msg.topic,
                    serde_json::from_str::<serde_json::Value>(msg.payload.as_str().expect("UTF8"))
                        .expect("JSON"),
                )
            })
            .unwrap();

        let mqtt_schema = MqttSchema::default();
        let (entity, channel) = mqtt_schema.entity_channel_of(&topic).unwrap();
        assert_eq!(entity, "device/main//");

        if let Channel::Command {
            operation: OperationType::LogUpload,
            cmd_id,
        } = channel
        {
            // Validate the topic name
            assert_eq!(
                topic.name,
                format!("te/device/main///cmd/log_upload/{cmd_id}")
            );

            // Validate the payload JSON
            let expected_json = json!({
                "status": "init",
                "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/test-device/log_upload/logfileA-{cmd_id}"),
                "type": "logfileA",
                "dateFrom": "2013-06-22T17:03:14.123+02:00",
                "dateTo": "2013-06-23T18:03:14.123+02:00",
                "searchText": "ERROR",
                "lines": 1000
            });

            assert_json_diff::assert_json_include!(actual: received_json, expected: expected_json);
        } else {
            panic!("Unexpected response on channel: {:?}", topic)
        }
    }

    #[tokio::test]
    async fn mapper_converts_smartrest_logfile_req_to_log_upload_cmd_for_child_device() {
        let cfg_dir = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate log_upload cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/DeviceSerial///cmd/log_upload"),
            r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
        ))
        .await
        .expect("Send failed");

        mqtt.skip(3).await; //Skip entity registration, mapping and supported log types messages

        // Simulate c8y_LogfileRequest SmartREST request
        mqtt.send(MqttMessage::new(
            &C8yTopic::downstream_topic(),
            "522,test-device:device:DeviceSerial,logfileA,2013-06-22T17:03:14.123+02:00,2013-06-23T18:03:14.123+02:00,ERROR,1000",
        ))
            .await
            .expect("Send failed");

        let (topic, received_json) = mqtt
            .recv()
            .await
            .map(|msg| {
                (
                    msg.topic,
                    serde_json::from_str::<serde_json::Value>(msg.payload.as_str().expect("UTF8"))
                        .expect("JSON"),
                )
            })
            .unwrap();

        let mqtt_schema = MqttSchema::default();
        let (entity, channel) = mqtt_schema.entity_channel_of(&topic).unwrap();
        assert_eq!(entity, "device/DeviceSerial//");

        if let Channel::Command {
            operation: OperationType::LogUpload,
            cmd_id,
        } = channel
        {
            // Validate the topic name
            assert_eq!(
                topic.name,
                format!("te/device/DeviceSerial///cmd/log_upload/{cmd_id}")
            );

            // Validate the payload JSON
            let expected_json = json!({
                "status": "init",
                "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/test-device:device:DeviceSerial/log_upload/logfileA-{cmd_id}"),
                "type": "logfileA",
                "dateFrom": "2013-06-22T17:03:14.123+02:00",
                "dateTo": "2013-06-23T18:03:14.123+02:00",
                "searchText": "ERROR",
                "lines": 1000
            });

            assert_json_diff::assert_json_include!(actual: received_json, expected: expected_json);
        } else {
            panic!("Unexpected response on channel: {:?}", topic)
        }
    }

    #[tokio::test]
    async fn mapper_converts_log_upload_cmd_to_supported_op_and_types_for_main_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate log_upload cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/log_upload"),
            r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
        ))
        .await
        .expect("Send failed");

        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "118,typeA,typeB,typeC")]).await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/c8y_LogfileRequest")
            .exists());
    }

    #[tokio::test]
    async fn mapper_converts_log_upload_cmd_to_supported_op_and_types_for_child_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate log_upload cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/log_upload"),
            r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Expect auto-registration message
        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/child1//",
                json!({"@type":"child-device","@id":"test-device:device:child1"}),
            )],
        )
        .await;

        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us",
                "101,test-device:device:child1,child1,thin-edge.io-child",
            )],
        )
        .await;
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/test-device:device:child1",
                "118,typeA,typeB,typeC",
            )],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/test-device:device:child1/c8y_LogfileRequest")
            .exists());
    }

    #[tokio::test]
    async fn handle_log_upload_executing_and_failed_cmd_for_main_device() {
        let cfg_dir = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // Simulate log_upload command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/log_upload/c8y-mapper-1234"),
            json!({
            "status": "executing",
            "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/test-device/log_upload/typeA-c8y-mapper-1234"),
            "type": "typeA",
            "dateFrom": "2013-06-22T17:03:14.123+02:00",
            "dateTo": "2013-06-23T18:03:14.123+02:00",
            "searchText": "ERROR",
            "lines": 1000
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `501` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_LogfileRequest")]).await;

        // Simulate log_upload command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/log_upload/c8y-mapper-1234"),
            json!({
            "status": "failed",
            "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/test-device/log_upload/typeA-c8y-mapper-1234"),
            "type": "typeA",
            "dateFrom": "2013-06-22T17:03:14.123+02:00",
            "dateTo": "2013-06-23T18:03:14.123+02:00",
            "searchText": "ERROR",
            "lines": 1000
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `502` smartrest message on `c8y/s/us`.
        assert_received_contains_str(
            &mut mqtt,
            [("c8y/s/us", "502,c8y_LogfileRequest,Unknown reason")],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_log_upload_executing_and_failed_cmd_for_child_device() {
        let cfg_dir = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // Simulate log_upload command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/log_upload/c8y-mapper-1234"),
            json!({
            "status": "executing",
            "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/child1/log_upload/typeA-c8y-mapper-1234"),
            "type": "typeA",
            "dateFrom": "2013-06-22T17:03:14.123+02:00",
            "dateTo": "2013-06-23T18:03:14.123+02:00",
            "searchText": "ERROR",
            "lines": 1000
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect auto-registration message
        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/child1//",
                json!({"@type":"child-device","@id":"test-device:device:child1"}),
            )],
        )
        .await;

        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us",
                "101,test-device:device:child1,child1,thin-edge.io-child",
            )],
        )
        .await;

        // Expect `501` smartrest message on `c8y/s/us/child1`.
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/test-device:device:child1",
                "501,c8y_LogfileRequest",
            )],
        )
        .await;

        // Simulate log_upload command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/log_upload/c8y-mapper-1234"),
            json!({
            "status": "failed",
            "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/child1/log_upload/typeA-c8y-mapper-1234"),
            "type": "typeA",
            "dateFrom": "2013-06-22T17:03:14.123+02:00",
            "dateTo": "2013-06-23T18:03:14.123+02:00",
            "searchText": "ERROR",
            "lines": 1000,
            "reason": "Something went wrong"
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `502` smartrest message on `c8y/s/us/child1`.
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/test-device:device:child1",
                "502,c8y_LogfileRequest,Something went wrong",
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_log_upload_successful_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, http, _fs, _timer, ul, dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        spawn_dummy_c8y_http_proxy(http);

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        let mut ul = ul.with_timeout(TEST_TIMEOUT_MS);
        let mut dl = dl.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // Simulate log_upload command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/log_upload/c8y-mapper-1234"),
            json!({
            "status": "successful",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/log_upload/typeA-c8y-mapper-1234",
            "type": "typeA",
            "dateFrom": "2013-06-22T17:03:14.123+02:00",
            "dateTo": "2013-06-23T18:03:14.123+02:00",
            "searchText": "ERROR",
            "lines": 1000
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Downloader gets a download request
        let download_request = dl.recv().await.expect("timeout");
        assert_eq!(download_request.0, "c8y-mapper-1234"); // Command ID

        // simulate downloader returns result
        dl.send((
            download_request.0,
            Ok(DownloadResponse {
                url: download_request.1.url,
                file_path: download_request.1.file_path,
            }),
        ))
        .await
        .unwrap();

        // Uploader gets a upload request and assert that
        let request = ul.recv().await.expect("timeout");
        assert_eq!(request.0, "c8y-mapper-1234"); // Command ID
        assert_eq!(
            request.1.url,
            "http://127.0.0.1:8001/c8y/event/events/dummy-event-id-1234/binaries"
        );

        // Simulate Uploader returns a result
        ul.send((
            request.0,
            Ok(UploadResponse {
                url: request.1.url,
                file_path: request.1.file_path,
            }),
        ))
        .await
        .unwrap();

        // Expect `503` smartrest message on `c8y/s/us`.
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us",
                "503,c8y_LogfileRequest,https://test.c8y.io/event/events/dummy-event-id-1234/binaries",
            )],
        )
            .await;
    }

    #[tokio::test]
    async fn handle_log_upload_successful_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, http, _fs, _timer, ul, dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        spawn_dummy_c8y_http_proxy(http);

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        let mut ul = ul.with_timeout(TEST_TIMEOUT_MS);
        let mut dl = dl.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // Simulate log_upload command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/log_upload/c8y-mapper-1234"),
            json!({
            "status": "successful",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/child1/log_upload/typeA-c8y-mapper-1234",
            "type": "typeA",
            "dateFrom": "2013-06-22T17:03:14.123+02:00",
            "dateTo": "2013-06-23T18:03:14.123+02:00",
            "searchText": "ERROR",
            "lines": 1000
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        mqtt.skip(2).await; // Skip child device registration messages

        // Downloader gets a download request
        let download_request = dl.recv().await.expect("timeout");
        assert_eq!(download_request.0, "c8y-mapper-1234"); // Command ID

        // simulate downloader returns result
        dl.send((
            download_request.0,
            Ok(DownloadResponse {
                url: download_request.1.url,
                file_path: download_request.1.file_path,
            }),
        ))
        .await
        .unwrap();

        // Uploader gets a upload request and assert that
        let request = ul.recv().await.expect("timeout");
        assert_eq!(request.0, "c8y-mapper-1234"); // Command ID
        assert_eq!(
            request.1.url,
            "http://127.0.0.1:8001/c8y/event/events/dummy-event-id-1234/binaries"
        );

        // Simulate Uploader returns a result
        ul.send((
            request.0,
            Ok(UploadResponse {
                url: request.1.url,
                file_path: request.1.file_path,
            }),
        ))
        .await
        .unwrap();

        // Expect `503` smartrest message on `c8y/s/us`.
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/test-device:device:child1",
                "503,c8y_LogfileRequest,https://test.c8y.io/event/events/dummy-event-id-1234/binaries",
            )],
        )
            .await;
    }
}
