//! The operations executed by the mapper itself, running the command of an
//! operation file: for SmartREST templates, and for JSON custom operations
//! without a workflow

use super::CumulocityConverter;
use crate::error::ConversionError;
use crate::error::CumulocityMapperError;
use crate::supported_operations::operation::Operation;
use crate::supported_operations::operation::ResultFormat;
use c8y_api::smartrest::message::collect_smartrest_messages;
use c8y_api::smartrest::message::get_failure_reason_for_smartrest;
use c8y_api::smartrest::message::get_smartrest_device_id;
use c8y_api::smartrest::message::get_smartrest_template_id;
use c8y_api::smartrest::message::MAX_PAYLOAD_LIMIT_IN_BYTES;
use c8y_api::smartrest::payload::SmartrestPayload;
use c8y_api::smartrest::smartrest_serializer::EmbeddedCsv;
use c8y_api::smartrest::smartrest_serializer::OperationReference;
use c8y_api::smartrest::smartrest_serializer::TextOrCsv;
use c8y_api::smartrest::topic::C8yTopic;
use std::process::Output;
use std::str::FromStr;
use tedge_actors::LoggingSender;
use tedge_actors::Sender;
use tedge_api::script::ShellScript;
use tedge_api::workflow::GenericCommandState;
use tedge_api::LoggedCommand;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tracing::debug;
use tracing::error;
use tracing::Instrument;

impl CumulocityConverter {
    /// Runs the operations received as SmartREST messages, for the templates of the
    /// operation files
    pub(super) async fn run_smartrest_operations(
        &mut self,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut output: Vec<MqttMessage> = Vec::new();
        for smartrest_message in collect_smartrest_messages(message.payload_str()?) {
            let result = self.process_smartrest(smartrest_message.as_str()).await;
            let mut msgs = self.operation_result_messages(
                result,
                self.entity_cache.main_device_external_id().as_ref(),
                None,
            );
            output.append(&mut msgs)
        }
        Ok(output)
    }

    /// Runs the command of a JSON custom operation that has no workflow
    pub(super) async fn execute_custom_operation(
        &self,
        custom_handler: &Operation,
        message: &MqttMessage,
        operation_id: &str,
    ) -> Result<(), CumulocityMapperError> {
        let handler_error =
            |err_msg: String| CumulocityMapperError::JsonCustomOperationHandlerError {
                operation: custom_handler.name.clone(),
                err_msg,
            };
        let state = GenericCommandState::from_command_message(message).map_err(|e| {
            handler_error(format!("Invalid JSON message, {e}. Message: {message:?}"))
        })?;
        let command_value = custom_handler
            .command()
            .ok_or_else(|| handler_error("'command' is missing".to_owned()))?;
        let script_template = ShellScript::from_str(&command_value)
            .map_err(|e| handler_error(format!("Fail to parse the script {command_value}: {e}")))?;
        let script = script_template.inject_values(&state);

        self.execute_operation(script, custom_handler, Some(operation_id.into()))
            .await
    }

    /// Runs the command of the operation file matching a SmartREST template, for
    /// the main device only
    async fn process_smartrest(
        &mut self,
        payload: &str,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        match get_smartrest_device_id(payload) {
            Some(device_id) if device_id == self.config.device_id => {
                let template = get_smartrest_template_id(payload);
                let Some(operation) = self
                    .supported_operations
                    .matching_smartrest_template(&template)
                else {
                    return Ok(vec![]);
                };
                if let Some(command) = operation.command() {
                    let script = ShellScript {
                        command,
                        args: vec![payload.to_owned()],
                    };
                    self.execute_operation(script, operation, None).await?;
                }
                // The statuses are published by the task that execute_operation spawns
                Ok(vec![])
            }
            // Child devices are not supported yet
            _ => {
                debug!("Ignored. Message not yet supported: {payload}");
                Ok(vec![])
            }
        }
    }

    /// Spawns the operation's command and returns once it is running. Unless the
    /// operation skips them, a background task publishes the executing and final
    /// statuses.
    ///
    /// # Errors
    /// Returns an error if the command or its log file cannot be created
    pub(super) async fn execute_operation(
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

    // Not truncated here, as the characters rejected by Cumulocity are removed
    // before the result is truncated to fit the message
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::converter::tests::create_c8y_converter;
    use camino::Utf8Path;
    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn no_log_file_is_created_for_a_command_that_cannot_be_spawned() {
        let tmp_dir = TempTedgeDir::new();
        let (converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        let mut operation =
            toml::from_str::<Operation>("[exec]\ncommand = \"/nonexistent/command\"").unwrap();
        operation.name = "missing_command".to_owned();

        let result = converter
            .execute_operation(
                ShellScript::from_str("/nonexistent/command").unwrap(),
                &operation,
                None,
            )
            .await;

        assert!(
            matches!(result, Err(CumulocityMapperError::ExecuteFailed { .. })),
            "{result:?}"
        );
        assert_eq!(
            log_files(tmp_dir.path(), "missing_command-"),
            Vec::<String>::new()
        );
    }

    /// The files in the given directory and its subdirectories whose names start
    /// with the given prefix
    fn log_files(dir: &Utf8Path, prefix: &str) -> Vec<String> {
        let mut found = vec![];
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                found.extend(log_files(Utf8Path::from_path(&path).unwrap(), prefix));
            } else if name.starts_with(prefix) {
                found.push(path.display().to_string());
            }
        }
        found
    }
}
