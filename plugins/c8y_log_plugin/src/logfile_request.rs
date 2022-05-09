use std::path::Path;

use glob::glob;

use crate::config::LogPluginConfig;
use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use c8y_smartrest::{
    smartrest_deserializer::{get_datetime_from_file_path, SmartRestLogRequest},
    smartrest_serializer::{
        CumulocitySupportedOperations, SmartRestSerializer, SmartRestSetOperationToExecuting,
        SmartRestSetOperationToFailed, SmartRestSetOperationToSuccessful,
        TryIntoOperationStatusMessage,
    },
};
use mqtt_channel::{Connection, SinkExt};

pub struct LogfileRequest {}

impl TryIntoOperationStatusMessage for LogfileRequest {
    /// returns a c8y message specifying to set log status to executing.
    ///
    /// example message: '501,c8y_LogfileRequest'
    fn status_executing() -> Result<
        c8y_smartrest::smartrest_serializer::SmartRest,
        c8y_smartrest::error::SmartRestSerializerError,
    > {
        SmartRestSetOperationToExecuting::new(CumulocitySupportedOperations::C8yLogFileRequest)
            .to_smartrest()
    }

    fn status_successful(
        parameter: Option<String>,
    ) -> Result<
        c8y_smartrest::smartrest_serializer::SmartRest,
        c8y_smartrest::error::SmartRestSerializerError,
    > {
        SmartRestSetOperationToSuccessful::new(CumulocitySupportedOperations::C8yLogFileRequest)
            .with_response_parameter(&parameter.unwrap())
            .to_smartrest()
    }

    fn status_failed(
        failure_reason: String,
    ) -> Result<
        c8y_smartrest::smartrest_serializer::SmartRest,
        c8y_smartrest::error::SmartRestSerializerError,
    > {
        SmartRestSetOperationToFailed::new(
            CumulocitySupportedOperations::C8yLogFileRequest,
            failure_reason,
        )
        .to_smartrest()
    }
}
/// Reads tedge logs according to `SmartRestLogRequest`.
///
/// If needed, logs are concatenated.
///
/// Logs are sorted alphanumerically from oldest to newest.
///
/// # Examples
///
/// ```
/// let smartrest_obj = SmartRestLogRequest::from_smartrest(
///     "522,DeviceSerial,syslog,2021-01-01T00:00:00+0200,2021-01-10T00:00:00+0200,,1000",
/// )
/// .unwrap();
///
/// let log = read_tedge_system_logs(&smartrest_obj, "/var/log/tedge").unwrap();
/// ```
pub fn read_tedge_logs(
    smartrest_obj: &SmartRestLogRequest,
    plugin_config_path: &Path,
) -> Result<String, anyhow::Error> {
    let plugin_config = LogPluginConfig::new(&plugin_config_path);
    let mut output = String::new();

    let mut files_to_send = Vec::new();
    for files in &plugin_config.files {
        let maybe_file_path = files.path.as_str(); // because it can be a glob pattern
        let file_type = files.config_type.as_str();

        if !file_type.eq(&smartrest_obj.log_type) {
            continue;
        }

        // NOTE: According to the glob documentation paths are yielded in alphabetical order hence re-ordering is no longer required see:
        // https://github.com/thin-edge/thin-edge.io/blob/0320741b109f50d1b0f7cda44e33dc31ba04902d/plugins/log_request_plugin/src/smartrest.rs#L24
        for entry in glob(maybe_file_path)? {
            let file_path = entry?;
            if let Some(dt_from_file) = get_datetime_from_file_path(&file_path) {
                if !(dt_from_file < smartrest_obj.date_from || dt_from_file > smartrest_obj.date_to)
                {
                    files_to_send.push(file_path);
                }
            } else {
                files_to_send.push(file_path);
            }
        }
    }

    // loop sorted vector and push store log file to `output`
    let mut line_counter: usize = 0;
    for entry in files_to_send {
        dbg!("files to read:", &entry);
        let file_content = std::fs::read_to_string(&entry)?;
        if file_content.is_empty() {
            continue;
        }

        // adding file header only if line_counter permits more lines to be added
        match &entry.file_stem().and_then(|f| f.to_str()) {
            Some(file_name) if line_counter < smartrest_obj.lines => {
                output.push_str(&format!("filename: {}\n", file_name));
            }
            _ => {}
        }

        // split at new line delimiter ("\n")
        let mut lines = file_content.lines().rev();
        while line_counter < smartrest_obj.lines {
            if let Some(haystack) = lines.next() {
                if let Some(needle) = &smartrest_obj.needle {
                    if haystack.contains(needle) {
                        output.push_str(&format!("{}\n", haystack));
                        line_counter += 1;
                    }
                } else {
                    output.push_str(&format!("{}\n", haystack));
                    line_counter += 1;
                }
            } else {
                // there are no lines.next()
                break;
            }
        }
    }
    Ok(output)
}

pub async fn handle_logfile_request_operation(
    smartrest_request: &SmartRestLogRequest,
    plugin_config_path: &Path,
    mqtt_client: &mut Connection,
    http_client: &mut JwtAuthHttpProxy,
) -> Result<(), anyhow::Error> {
    // executing
    let executing = LogfileRequest::executing()?;
    let () = mqtt_client.published.send(executing).await?;

    let log_content = read_tedge_logs(&smartrest_request, &plugin_config_path)?;

    let upload_event_url = http_client
        .upload_log_binary(&smartrest_request.log_type, &log_content)
        .await?;

    let successful = LogfileRequest::successful(Some(upload_event_url))?;
    let () = mqtt_client.published.send(successful).await?;

    Ok(())
}

pub async fn handle_dynamic_log_type_update(
    mqtt_client: &mut Connection,
    config_dir: &Path,
) -> Result<(), anyhow::Error> {
    let plugin_config = LogPluginConfig::new(config_dir);
    let msg = plugin_config.to_supported_config_types_message()?;
    let () = mqtt_client.published.send(msg).await?;
    Ok(())
}
