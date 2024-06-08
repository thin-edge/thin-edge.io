use super::FtsDownloadOperationData;
use crate::actor::CmdId;
use crate::converter::CumulocityConverter;
use crate::converter::UploadOperationData;
use crate::error::ConversionError;
use crate::error::CumulocityMapperError;
use crate::operations::FtsDownloadOperationType;
use anyhow::Context;
use c8y_api::json_c8y_deserializer::C8yUploadConfigFile;
use c8y_api::smartrest::smartrest_serializer::fail_operation;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use camino::Utf8PathBuf;
use tedge_actors::Sender;
use tedge_api::commands::CommandStatus;
use tedge_api::commands::ConfigSnapshotCmd;
use tedge_api::commands::ConfigSnapshotCmdPayload;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandState;
use tedge_api::Jsonify;
use tedge_downloader_ext::DownloadRequest;
use tedge_downloader_ext::DownloadResult;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::TopicFilter;
use tracing::log::warn;

pub fn topic_filter(mqtt_schema: &MqttSchema) -> TopicFilter {
    [
        mqtt_schema.topics(
            EntityFilter::AnyEntity,
            ChannelFilter::Command(OperationType::ConfigSnapshot),
        ),
        mqtt_schema.topics(
            EntityFilter::AnyEntity,
            ChannelFilter::CommandMetadata(OperationType::ConfigSnapshot),
        ),
    ]
    .into_iter()
    .collect()
}

impl CumulocityConverter {
    /// Convert c8y_UploadConfigFile JSON over MQTT operation to ThinEdge config_snapshot command
    pub fn convert_config_snapshot_request(
        &self,
        device_xid: String,
        cmd_id: String,
        config_upload_request: C8yUploadConfigFile,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let target = self
            .entity_store
            .try_get_by_external_id(&device_xid.into())?;

        let channel = Channel::Command {
            operation: OperationType::ConfigSnapshot,
            cmd_id: cmd_id.clone(),
        };
        let topic = self.mqtt_schema.topic_for(&target.topic_id, &channel);

        // Replace '/' with ':' to avoid creating unexpected directories in file transfer repo
        let tedge_url = format!(
            "http://{}/tedge/file-transfer/{}/config_snapshot/{}-{}",
            &self.config.tedge_http_host,
            target.external_id.as_ref(),
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

    /// Address received ThinEdge config_snapshot command. If its status is
    /// - "executing", it converts the message to SmartREST "Executing".
    /// - "successful", it uploads a config snapshot to c8y and converts the message to SmartREST "Successful".
    /// - "failed", it converts the message to SmartREST "Failed".
    pub async fn handle_config_snapshot_state_change(
        &mut self,
        topic_id: &EntityTopicId,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<(Vec<MqttMessage>, Option<GenericCommandState>), ConversionError> {
        if !self.config.capabilities.config_snapshot {
            warn!(
                "Received a config_snapshot command, however, config_snapshot feature is disabled"
            );
            return Ok((vec![], None));
        }

        let command = match ConfigSnapshotCmd::try_from_bytes(
            topic_id.clone(),
            cmd_id.into(),
            message.payload_bytes(),
        )? {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok((vec![], None));
            }
        };

        let target = self.entity_store.try_get(topic_id)?;
        let smartrest_topic = self.smartrest_publish_topic_for_entity(topic_id)?;

        let messages = match command.status() {
            CommandStatus::Executing => {
                let smartrest_operation_status =
                    set_operation_executing(CumulocitySupportedOperations::C8yUploadConfigFile);
                vec![MqttMessage::new(
                    &smartrest_topic,
                    smartrest_operation_status,
                )]
            }
            CommandStatus::Successful => {
                // Send a request to the Downloader to download the file asynchronously from FTS
                let config_filename = format!(
                    "{}-{}",
                    command.payload.config_type.replace('/', ":"),
                    cmd_id
                );

                let tedge_file_url = format!(
                    "http://{}/tedge/file-transfer/{external_id}/config_snapshot/{config_filename}",
                    &self.config.tedge_http_host,
                    external_id = target.external_id.as_ref()
                );

                let destination_dir = tempfile::tempdir_in(self.config.tmp_dir.as_std_path())
                    .context("Failed to create a temporary directory")?;
                let destination_path = destination_dir.path().join(config_filename);

                self.pending_fts_download_operations.insert(
                    cmd_id.into(),
                    FtsDownloadOperationData {
                        download_type: FtsDownloadOperationType::ConfigDownload,
                        url: tedge_file_url.clone(),
                        file_dir: destination_dir,

                        message: message.clone(),
                        entity_topic_id: topic_id.clone(),
                        command: command.clone().into_generic_command(&self.mqtt_schema),
                    },
                );

                let download_request = DownloadRequest::new(&tedge_file_url, &destination_path);

                self.downloader_sender
                    .send((cmd_id.into(), download_request))
                    .await
                    .map_err(CumulocityMapperError::ChannelError)?;

                // cont. in handle_fts_config_download_result

                vec![] // No mqtt message can be published in this state
            }
            CommandStatus::Failed { reason } => {
                let smartrest_operation_status =
                    fail_operation(CumulocitySupportedOperations::C8yUploadConfigFile, &reason);
                let c8y_notification =
                    MqttMessage::new(&smartrest_topic, smartrest_operation_status);
                let clear_local_cmd = MqttMessage::new(&message.topic, "")
                    .with_retain()
                    .with_qos(QoS::AtLeastOnce);
                vec![c8y_notification, clear_local_cmd]
            }
            _ => {
                vec![] // Do nothing as other components might handle those states
            }
        };

        Ok((
            messages,
            Some(command.into_generic_command(&self.mqtt_schema)),
        ))
    }

    /// Resumes `config_snapshot` operation after required file was downloaded
    /// from the File Transfer Service.
    pub async fn handle_fts_config_download_result(
        &mut self,
        cmd_id: CmdId,
        download_result: DownloadResult,
        fts_download: FtsDownloadOperationData,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let topic_id = fts_download.entity_topic_id;
        let smartrest_topic = self.smartrest_publish_topic_for_entity(&topic_id)?;
        let payload = fts_download.message.payload_str()?;
        let response = &ConfigSnapshotCmdPayload::from_json(payload)?;

        let download = match download_result {
            Err(err) => {
                let smartrest_error =
                    fail_operation(
                        CumulocitySupportedOperations::C8yUploadConfigFile,
                        &format!("tedge-mapper-c8y failed to download configuration snapshot from file-transfer service: {err}"),
                    );

                let c8y_notification = MqttMessage::new(&smartrest_topic, smartrest_error);
                let clean_operation = MqttMessage::new(&fts_download.message.topic, "")
                    .with_retain()
                    .with_qos(QoS::AtLeastOnce);

                return Ok(vec![c8y_notification, clean_operation]);
            }
            Ok(download) => download,
        };

        let file_path = Utf8PathBuf::try_from(download.file_path).map_err(|e| e.into_io_error())?;
        let event_type = response.config_type.clone();

        let binary_upload_event_url = self
            .upload_file(&topic_id, &file_path, None, None, &cmd_id, event_type, None)
            .await?;

        self.pending_upload_operations.insert(
            cmd_id.clone(),
            UploadOperationData {
                topic_id,
                file_dir: fts_download.file_dir,
                smartrest_topic,
                clear_cmd_topic: fts_download.message.topic,
                c8y_binary_url: binary_upload_event_url.to_string(),
                operation: CumulocitySupportedOperations::C8yUploadConfigFile,
                command: fts_download.command,
            }
            .into(),
        );

        Ok(vec![])
    }

    /// Converts a config_snapshot metadata message to
    /// - supported operation "c8y_UploadConfigFile"
    /// - supported config types
    pub fn convert_config_snapshot_metadata(
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
    }
}

#[cfg(test)]
mod tests {
    use crate::config::C8yMapperConfig;
    use crate::tests::skip_init_messages;
    use crate::tests::spawn_c8y_mapper_actor;
    use crate::tests::spawn_c8y_mapper_actor_with_config;
    use crate::tests::spawn_dummy_c8y_http_proxy;
    use crate::tests::test_mapper_config;
    use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
    use serde_json::json;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::MessageReceiver;
    use tedge_actors::Sender;
    use tedge_config::AutoLogUpload;
    use tedge_downloader_ext::DownloadResponse;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::test_helpers::assert_received_includes_json;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;
    use tedge_uploader_ext::UploadResponse;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

    #[tokio::test]
    async fn mapper_converts_config_upload_op_to_config_snapshot_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_UploadConfigFile operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "c8y_UploadConfigFile": {
                    "type": "path/config/A"
                },
                "externalSource": {
                    "externalId": "test-device",
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/main///cmd/config_snapshot/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/config_snapshot/path:config:A-c8y-mapper-123456",
                    "type": "path/config/A",
                }),
            )],
        )
            .await;
    }

    #[tokio::test]
    async fn mapper_converts_config_upload_op_to_config_snapshot_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // The child device must be registered first
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            r#"{ "@type":"child-device", "@id":"child1" }"#,
        ))
        .await
        .expect("fail to register the child-device");

        mqtt.skip(1).await; // Skip the mapped child device registration message

        // Simulate c8y_UploadConfigFile operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "c8y_UploadConfigFile": {
                    "type": "configA"
                },
                "externalSource": {
                    "externalId": "child1",
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/child1///cmd/config_snapshot/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "tedgeUrl": "http://localhost:8888/tedge/file-transfer/child1/config_snapshot/configA-c8y-mapper-123456",
                    "type": "configA",
                }),
            )],
        )
            .await;
    }

    #[tokio::test]
    async fn handle_config_snapshot_executing_and_failed_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate config_snapshot command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_snapshot/c8y-mapper-1234"),
            json!({
            "status": "executing",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/config_snapshot/typeA-c8y-mapper-1234",
            "type": "typeA",
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `501` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_UploadConfigFile")]).await;

        // Simulate config_snapshot command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_snapshot/c8y-mapper-1234"),
            json!({
            "status": "failed",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/config_snapshot/typeA-c8y-mapper-1234",
            "type": "typeA",
            "reason": "Something went wrong"
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `502` smartrest message on `c8y/s/us`.
        assert_received_contains_str(
            &mut mqtt,
            [("c8y/s/us", "502,c8y_UploadConfigFile,Something went wrong")],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_config_snapshot_executing_and_failed_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // The child device must be registered first
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            r#"{ "@type":"child-device", "@id":"child1" }"#,
        ))
        .await
        .expect("fail to register the child-device");

        mqtt.skip(1).await; // Skip child device registration messages

        // Simulate config_snapshot command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_snapshot/c8y-mapper-1234"),
            json!({
            "status": "executing",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/child1/config_snapshot/typeA-c8y-mapper-1234",
            "type": "typeA",
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `501` smartrest message on child topic.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us/child1", "501,c8y_UploadConfigFile")])
            .await;

        // Simulate config_snapshot command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_snapshot/c8y-mapper-1234"),
            json!({
            "status": "failed",
            "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/child1/config_snapshot/typeA-c8y-mapper-1234"),
            "type": "typeA",
            "reason": "Something went wrong"
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `502` smartrest message on child topic.
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/child1",
                "502,c8y_UploadConfigFile,Something went wrong",
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_config_snapshot_successful_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, http, _fs, _timer, ul, dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        spawn_dummy_c8y_http_proxy(http);

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        let mut ul = ul.with_timeout(TEST_TIMEOUT_MS);
        let mut dl = dl.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // Simulate config_snapshot command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_snapshot/c8y-mapper-1234"),
            json!({
            "status": "successful",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/config_snapshot/path:type:A-c8y-mapper-1234",
            "type": "path/type/A",
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

        // Uploader gets a download request and assert them
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
            [("c8y/s/us", "503,c8y_UploadConfigFile,https://test.c8y.io/event/events/dummy-event-id-1234/binaries")],
        )
            .await;
    }

    #[tokio::test]
    async fn handle_config_snapshot_successful_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, http, _fs, _timer, ul, dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        spawn_dummy_c8y_http_proxy(http);

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        let mut ul = ul.with_timeout(TEST_TIMEOUT_MS);
        let mut dl = dl.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // The child device must be registered first
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            r#"{ "@type":"child-device", "@id":"child1" }"#,
        ))
        .await
        .expect("fail to register the child-device");

        mqtt.skip(1).await; // Skip child device registration messages

        // Simulate config_snapshot command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_snapshot/c8y-mapper-1234"),
            json!({
            "status": "successful",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/child1/config_snapshot/typeA-c8y-mapper-1234",
            "type": "typeA",
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

        // Uploader gets a download request and assert them
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

        // Expect `503` smartrest message on child topic.
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/child1",
                "503,c8y_UploadConfigFile,https://test.c8y.io/event/events/dummy-event-id-1234/binaries",
            )],
        )
            .await;
    }

    #[tokio::test]
    async fn auto_log_upload_successful_operation() {
        let ttd = TempTedgeDir::new();
        let config = C8yMapperConfig {
            auto_log_upload: AutoLogUpload::Always,
            ..test_mapper_config(&ttd)
        };
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
        spawn_dummy_c8y_http_proxy(test_handle.c8y_http_box);

        let mut mqtt = test_handle.mqtt_box.with_timeout(TEST_TIMEOUT_MS);
        let mut ul = test_handle.ul_box.with_timeout(TEST_TIMEOUT_MS);
        let mut dl = test_handle.dl_box.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        let test_log = ttd.file("test.log");
        // Simulate config_snapshot command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_snapshot/c8y-mapper-1234"),
            json!({
            "status": "successful",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/config_snapshot/path:type:A-c8y-mapper-1234",
            "type": "path/type/A",
            "logPath": test_log.path()
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Downloader gets a download request
        let download_request = dl.recv().await.expect("timeout");
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

        // Uploader gets the upload request for the config file
        let request = ul.recv().await.expect("timeout");
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

        // Uploader gets the upload request for the log path
        let request = ul.recv().await.expect("timeout");
        assert_eq!(request.0, "c8y-mapper-1234"); // Command ID
        assert_eq!(request.1.file_path, test_log.utf8_path());

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
            [
                ("c8y/s/us", "503,c8y_UploadConfigFile,https://test.c8y.io/event/events/dummy-event-id-1234/binaries"), 
                ("te/device/main///cmd/config_snapshot/c8y-mapper-1234", ""),
            ],
        )
            .await;
    }

    #[tokio::test]
    async fn auto_log_upload_failed_operation() {
        let ttd = TempTedgeDir::new();
        let config = C8yMapperConfig {
            auto_log_upload: AutoLogUpload::Always,
            ..test_mapper_config(&ttd)
        };
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
        spawn_dummy_c8y_http_proxy(test_handle.c8y_http_box);

        let mut mqtt = test_handle.mqtt_box.with_timeout(TEST_TIMEOUT_MS);
        let mut ul = test_handle.ul_box.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        let test_log = ttd.file("test.log");
        // Simulate config_snapshot command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_snapshot/c8y-mapper-1234"),
            json!({
                "status": "failed",
                "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/config_snapshot/typeA-c8y-mapper-1234",
                "type": "typeA",
                "reason": "Something went wrong",
                "logPath": test_log.path(),
            }).to_string(),
        ))
        .await
        .expect("Send failed");

        // Uploader gets the upload request for the log path
        let request = ul.recv().await.expect("timeout");
        assert_eq!(request.0, "c8y-mapper-1234"); // Command ID
        assert_eq!(request.1.file_path, test_log.utf8_path());

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

        // Expect `502` smartrest message on `c8y/s/us`.
        assert_received_contains_str(
            &mut mqtt,
            [("c8y/s/us", "502,c8y_UploadConfigFile,Something went wrong")],
        )
        .await;
    }
}
