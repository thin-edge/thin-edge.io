use super::error::OperationError;
use super::EntityTarget;
use super::OperationContext;
use super::OperationOutcome;
use anyhow::Context;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use camino::Utf8PathBuf;
use tedge_api::commands::CommandStatus;
use tedge_api::commands::LogUploadCmd;
use tedge_api::mqtt_topics::OperationType;
use tedge_downloader_ext::DownloadRequest;
use tedge_mqtt_ext::MqttMessage;
use tracing::log::warn;

impl OperationContext {
    /// Address a received log_upload command. If its status is
    /// - "executing", it converts the message to SmartREST "Executing".
    /// - "successful", it creates an event in c8y, then creates an UploadRequest for the uploader actor.
    /// - "failed", it converts the message to SmartREST "Failed" with that event URL.
    pub async fn handle_log_upload_state_change(
        &self,
        target: &EntityTarget,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<OperationOutcome, OperationError> {
        if !self.capabilities.log_upload {
            warn!("Received a log_upload command, however, log_upload feature is disabled");
            return Ok(OperationOutcome::Ignored);
        }

        let command = match LogUploadCmd::try_from_bytes(
            target.topic_id.clone(),
            cmd_id.into(),
            message.payload_bytes(),
        )
        .context("Could not parse command as a log upload command")?
        {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok(OperationOutcome::Ignored);
            }
        };

        let smartrest_topic = &target.smartrest_publish_topic;

        match command.status() {
            CommandStatus::Executing => Ok(OperationOutcome::Executing {
                extra_messages: vec![],
            }),
            CommandStatus::Successful => {
                // Send a request to the Downloader to download the file asynchronously from FTS
                let log_filename = format!("{}-{}", command.payload.log_type, cmd_id);

                let tedge_file_url = &command.payload.tedge_url;

                let destination_dir = tempfile::tempdir_in(self.tmp_dir.as_std_path())
                    .context("Failed to create a temporary directory")?;
                let destination_path = destination_dir.path().join(log_filename);

                let download_request = DownloadRequest::new(tedge_file_url, &destination_path);
                let (_, download_result) = self
                    .downloader
                    .clone()
                    .await_response((cmd_id.into(), download_request))
                    .await
                    .context("Unexpected ChannelError")?;

                let download_response = download_result.context(
                    "tedge-mapper-c8y failed to download log from file transfer service",
                )?;

                let file_path = Utf8PathBuf::try_from(download_response.file_path)
                    .map_err(|e| e.into_io_error())
                    .context("Could not parse file path as Utf-8")?;

                let event_type = &command.payload.log_type;

                let (binary_upload_event_url, upload_result) = self
                    .upload_file(
                        &target.external_id,
                        &file_path,
                        None,
                        Some(mime::TEXT_PLAIN),
                        cmd_id,
                        event_type.clone(),
                        None,
                    )
                    .await
                    .context("Could not upload log file to C8y")?;

                let smartrest_response = super::get_smartrest_response_for_upload_result(
                    upload_result,
                    binary_upload_event_url.as_str(),
                    CumulocitySupportedOperations::C8yLogFileRequest,
                    self.smart_rest_use_operation_id,
                    self.get_operation_id(cmd_id),
                );

                let c8y_notification = MqttMessage::new(smartrest_topic, smartrest_response);

                self.upload_operation_log(
                    &target.external_id,
                    cmd_id,
                    &OperationType::LogUpload,
                    &command.clone().into_generic_command(&self.mqtt_schema),
                )
                .await
                .context("Could not upload operation log")?;

                Ok(OperationOutcome::Finished {
                    messages: vec![c8y_notification],
                })
            }
            CommandStatus::Failed { reason } => Err(anyhow::anyhow!(reason).into()),
            _ => {
                // Do nothing as other components might handle those states
                Ok(OperationOutcome::Ignored)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::C8yMapperConfig;
    use crate::tests::*;
    use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
    use serde_json::json;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::MessageReceiver;
    use tedge_actors::Sender;
    use tedge_downloader_ext::DownloadResponse;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::test_helpers::assert_received_includes_json;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;
    use tedge_uploader_ext::UploadResponse;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

    #[tokio::test]
    async fn mapper_converts_smartrest_logfile_req_to_log_upload_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;

        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_LogfileRequest JSON over MQTT request
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "c8y_LogfileRequest": {
                    "searchText": "ERROR",
                    "logFile": "logfileA",
                    "dateTo": "2023-11-29T16:33:50+0100",
                    "dateFrom": "2023-11-28T16:33:50+0100",
                    "maximumLines": 1000
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
                "te/device/main///cmd/log_upload/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/log_upload/logfileA-c8y-mapper-123456",
                    "type": "logfileA",
                    "dateFrom": "2023-11-28T16:33:50+01:00",
                    "dateTo": "2023-11-29T16:33:50+01:00",
                    "searchText": "ERROR",
                    "lines": 1000
                }),
            )],
        ).await;
    }

    #[tokio::test]
    async fn mapper_converts_smartrest_logfile_req_to_log_upload_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;

        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate log_upload cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/DeviceSerial///cmd/log_upload"),
            r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
        ))
        .await
        .expect("Send failed");

        mqtt.skip(4).await; //Skip entity registration, mapping, supported ops and supported log types messages

        // Simulate c8y_LogfileRequest JSON over MQTT request
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "c8y_LogfileRequest": {
                    "searchText": "ERROR",
                    "logFile": "logfileA",
                    "dateTo": "2023-11-29T16:33:50+0100",
                    "dateFrom": "2023-11-28T16:33:50+0100",
                    "maximumLines": 1000
                },
                "externalSource": {
                    "externalId": "test-device:device:DeviceSerial",
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
                "te/device/DeviceSerial///cmd/log_upload/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device:device:DeviceSerial/log_upload/logfileA-c8y-mapper-123456",
                    "type": "logfileA",
                    "dateFrom": "2023-11-28T16:33:50+01:00",
                    "dateTo": "2023-11-29T16:33:50+01:00",
                    "searchText": "ERROR",
                    "lines": 1000
                }),
            )],
        ).await;
    }

    #[tokio::test]
    async fn mapper_converts_log_upload_cmd_to_supported_op_and_types_for_main_device() {
        let ttd: TempTedgeDir = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate log_upload cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/log_upload"),
            r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
        ))
        .await
        .expect("Send failed");

        assert_received_contains_str(
            &mut mqtt,
            [
                ("c8y/s/us", "114,c8y_LogfileRequest"),
                ("c8y/s/us", "118,typeA,typeB,typeC"),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/c8y_LogfileRequest")
            .exists());
    }

    #[tokio::test]
    async fn mapper_converts_log_upload_cmd_to_supported_op_and_types_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
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
            [
                (
                    "c8y/s/us",
                    "101,test-device:device:child1,child1,thin-edge.io-child",
                ),
                (
                    "c8y/s/us/test-device:device:child1",
                    "114,c8y_LogfileRequest",
                ),
                (
                    "c8y/s/us/test-device:device:child1",
                    "118,typeA,typeB,typeC",
                ),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/test-device:device:child1/c8y_LogfileRequest")
            .exists());

        // Sending an updated list of log types
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/log_upload"),
            r#"{"types" : [ "typeB", "typeC", "typeD" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Assert that the updated log type list does not trigger a duplicate supported ops message
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/test-device:device:child1",
                "118,typeB,typeC,typeD",
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_log_upload_executing_and_failed_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;

        let TestHandle { mqtt, .. } = test_handle;

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
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;

        let TestHandle { mqtt, .. } = test_handle;

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
    async fn handle_log_upload_executing_and_failed_cmd_with_op_id() {
        let ttd = TempTedgeDir::new();
        let config = C8yMapperConfig {
            smartrest_use_operation_id: true,
            ..test_mapper_config(&ttd)
        };
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;

        let TestHandle { mqtt, .. } = test_handle;

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

        // Expect `504` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "504,1234")]).await;

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

        // Expect `505` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "505,1234,Unknown reason")]).await;
    }

    #[tokio::test]
    async fn handle_log_upload_successful_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle {
            mqtt, http, ul, dl, ..
        } = test_handle;
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
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle {
            mqtt, http, ul, dl, ..
        } = test_handle;
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

    #[tokio::test]
    async fn handle_log_upload_successful_cmd_with_id() {
        let ttd = TempTedgeDir::new();
        let config = C8yMapperConfig {
            smartrest_use_operation_id: true,
            ..test_mapper_config(&ttd)
        };
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
        let TestHandle {
            mqtt, http, ul, dl, ..
        } = test_handle;
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

        // Expect `506` smartrest message on `c8y/s/us`.
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us",
                "506,1234,https://test.c8y.io/event/events/dummy-event-id-1234/binaries",
            )],
        )
        .await;
    }
}
