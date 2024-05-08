//! Utilities for executing Cumulocity operations.
//!
//! C8y operations need some special handling by the C8y mapper, which needs to use the C8y HTTP
//! proxy to report on their progress. Additionally, while executing operations we often need to
//! send messages to different actors and wait for their results before continuing.
//!
//! The operations are always triggered remotely by Cumulocity, and a triggered operation must
//! always terminate in a success or failure. This status needs to be reported to Cumulocity.
//!
//! This module contains:
//! - data definitions of various states which are necessary to maintain in the mapper
//! - status and error handing utilities for reporting operation success/failure in different ways
//!   (MQTT, Smartrest)
//! - implementations of operations

use crate::converter::CumulocityConverter;
use crate::error::ConversionError;
use tedge_api::commands::ConfigMetadata;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::workflow::GenericCommandState;
use tedge_api::Jsonify;
use tedge_mqtt_ext::MqttMessage;
use tracing::error;

pub mod config_snapshot;
pub mod config_update;
pub mod firmware_update;
pub mod log_upload;

/// Represents a pending download performed by the downloader from the FTS.
///
/// Functions which download files from the tedge File Transfer Service as part of handling
/// operations (e.g. when performing `log_upload` or `config_snapshot`, the relevant file is
/// uploaded into FTS) will use this type for communicating with the Downloader actor.
pub struct FtsDownloadOperationData {
    pub download_type: FtsDownloadOperationType,
    pub url: String,

    // used to automatically remove the temporary file after operation is finished
    pub file_dir: tempfile::TempDir,

    // TODO: remove this message field since command is available
    // the message that triggered the operation
    pub message: MqttMessage,

    pub entity_topic_id: EntityTopicId,

    pub command: GenericCommandState,
}

/// Used to denote as type of what operation was the file downloaded from the FTS.
///
/// Used to dispatch download result to the correct operation handler.
pub enum FtsDownloadOperationType {
    LogDownload,
    ConfigDownload,
}

impl CumulocityConverter {
    fn convert_config_metadata(
        &mut self,
        topic_id: &EntityTopicId,
        message: &MqttMessage,
        c8y_op_name: &str,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let metadata = ConfigMetadata::from_json(message.payload_str()?)?;

        let mut messages = match self.register_operation(topic_id, c8y_op_name) {
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
        let payload = format!("119,{supported_config_types}");
        let sm_topic = self.smartrest_publish_topic_for_entity(topic_id)?;
        messages.push(MqttMessage::new(&sm_topic, payload));

        Ok(messages)
    }
}

fn get_smartrest_response_for_upload_result(
    upload_result: tedge_uploader_ext::UploadResult,
    binary_url: &str,
    operation: c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations,
) -> c8y_api::smartrest::smartrest_serializer::SmartRest {
    match upload_result {
        Ok(_) => c8y_api::smartrest::smartrest_serializer::succeed_static_operation(
            operation,
            Some(binary_url),
        ),
        Err(err) => c8y_api::smartrest::smartrest_serializer::fail_operation(
            operation,
            &format!("Upload failed with {err}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::skip_init_messages;
    use crate::tests::spawn_c8y_mapper_actor;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::Sender;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

    #[tokio::test]
    async fn mapper_converts_config_metadata_to_supported_op_and_types_for_main_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate config_snapshot cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_snapshot"),
            r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Validate SmartREST message is published
        assert_received_contains_str(
            &mut mqtt,
            [
                ("c8y/s/us", "114,c8y_UploadConfigFile"),
                ("c8y/s/us", "119,typeA,typeB,typeC"),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/c8y_UploadConfigFile")
            .exists());

        // Simulate config_update cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_update"),
            r#"{"types" : [ "typeD", "typeE", "typeF" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Validate SmartREST message is published
        assert_received_contains_str(
            &mut mqtt,
            [
                (
                    "c8y/s/us",
                    "114,c8y_DownloadConfigFile,c8y_UploadConfigFile",
                ),
                ("c8y/s/us", "119,typeD,typeE,typeF"),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/c8y_DownloadConfigFile")
            .exists());
    }

    #[tokio::test]
    async fn mapper_converts_config_cmd_to_supported_op_and_types_for_child_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate config_snapshot cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_snapshot"),
            r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
        ))
        .await
        .expect("Send failed");

        mqtt.skip(2).await; // Skip the mapped child device registration message

        // Validate SmartREST message is published
        assert_received_contains_str(
            &mut mqtt,
            [
                (
                    "c8y/s/us/test-device:device:child1",
                    "114,c8y_UploadConfigFile",
                ),
                (
                    "c8y/s/us/test-device:device:child1",
                    "119,typeA,typeB,typeC",
                ),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/test-device:device:child1/c8y_UploadConfigFile")
            .exists());

        // Sending an updated list of config types
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_snapshot"),
            r#"{"types" : [ "typeB", "typeC", "typeD" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Assert that the updated config type list does not trigger a duplicate supported ops message
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/test-device:device:child1",
                "119,typeB,typeC,typeD",
            )],
        )
        .await;

        // Simulate config_update cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_update"),
            r#"{"types" : [ "typeD", "typeE", "typeF" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Validate SmartREST message is published
        assert_received_contains_str(
            &mut mqtt,
            [
                (
                    "c8y/s/us/test-device:device:child1",
                    "114,c8y_DownloadConfigFile,c8y_UploadConfigFile",
                ),
                (
                    "c8y/s/us/test-device:device:child1",
                    "119,typeD,typeE,typeF",
                ),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/test-device:device:child1/c8y_DownloadConfigFile")
            .exists());

        // Sending an updated list of config types
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_update"),
            r#"{"types" : [ "typeB", "typeC", "typeD" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Assert that the updated config type list does not trigger a duplicate supported ops message
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/test-device:device:child1",
                "119,typeB,typeC,typeD",
            )],
        )
        .await;
    }
}
