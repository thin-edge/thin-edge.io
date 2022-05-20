use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

use easy_reader::EasyReader;
use glob::glob;
use time::OffsetDateTime;

use crate::config::LogPluginConfig;
use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use c8y_smartrest::{
    smartrest_deserializer::SmartRestLogRequest,
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

fn read_log_content(
    logfile: &Path,
    mut line_counter: usize,
    max_lines: usize,
    filter_text: &Option<String>,
) -> Result<(usize, String), anyhow::Error> {
    if line_counter >= max_lines {
        Err(anyhow::anyhow!(
            "`max_lines` filled. No more logs to return."
        ))
    } else {
        let mut file_content_as_vec = VecDeque::new();
        let file = std::fs::File::open(&logfile)?;
        let file_name = format!(
            "filename: {}\n",
            logfile.file_name().unwrap().to_str().unwrap()
        );
        let mut reader = EasyReader::new(file)?;
        reader.eof();

        while line_counter < max_lines {
            if let Some(haystack) = reader.prev_line()? {
                if let Some(needle) = &filter_text {
                    if haystack.contains(needle) {
                        file_content_as_vec.push_front(format!("{}\n", haystack));
                        line_counter += 1;
                    }
                } else {
                    file_content_as_vec.push_front(format!("{}\n", haystack));
                    line_counter += 1;
                }
            } else {
                // there are no more lines.prev_line()
                break;
            }
        }

        file_content_as_vec.push_front(file_name);

        let file_content = file_content_as_vec
            .iter()
            .map(|x| x.to_string())
            .collect::<String>();
        Ok((line_counter, file_content))
    }
}

/// read any log file comming from `smartrest_obj.log_type`
pub fn new_read_logs(
    smartrest_obj: &SmartRestLogRequest,
    plugin_config: &LogPluginConfig,
) -> Result<String, anyhow::Error> {
    let mut output = String::new();
    // first filter logs on type
    let mut logfiles_to_read = filter_logs_on_type(&smartrest_obj, &plugin_config)?;
    logfiles_to_read = filter_logs_path_on_metadata(&smartrest_obj, logfiles_to_read)?;

    let mut line_counter = 0usize;
    for logfile in logfiles_to_read {
        match read_log_content(
            logfile.as_path(),
            line_counter,
            smartrest_obj.lines,
            &smartrest_obj.needle,
        ) {
            Ok((lines, file_content)) => {
                line_counter = lines;
                output.push_str(&file_content);
            }
            Err(_e) => {
                // TODO filter this error for `max_lines` error only
                break;
            }
        };
    }

    Ok(output)
}

fn filter_logs_on_type(
    smartrest_obj: &SmartRestLogRequest,
    plugin_config: &LogPluginConfig,
) -> Result<Vec<PathBuf>, anyhow::Error> {
    let mut files_to_send = Vec::new();
    for files in &plugin_config.files {
        let maybe_file_path = files.path.as_str(); // because it can be a glob pattern
        let file_type = files.config_type.as_str();

        if !file_type.eq(&smartrest_obj.log_type) {
            continue;
        } else {
            for entry in glob(maybe_file_path)? {
                let file_path = entry?;
                files_to_send.push(file_path)
            }
        }
    }
    Ok(files_to_send)
}

/// filter a vector of pathbufs according to `smartrest_obj.date_from` and `smartrest_obj.date_to`
fn filter_logs_path_on_metadata(
    smartrest_obj: &SmartRestLogRequest,
    logs_path_vec: Vec<PathBuf>,
) -> Result<Vec<PathBuf>, anyhow::Error> {
    let mut out = vec![];
    for file_pathbuf in logs_path_vec {
        let metadata = std::fs::metadata(&file_pathbuf)?;
        let datetime_modified = OffsetDateTime::from(metadata.modified()?);
        if datetime_modified >= smartrest_obj.date_from {
            out.push(file_pathbuf);
        }
    }
    Ok(out)
}

/// executes the log file request
///
/// - sends request executing (mqtt)
/// - uploads log content (http)
/// - sends request successful (mqtt)
pub async fn handle_logfile_request_operation(
    smartrest_request: &SmartRestLogRequest,
    plugin_config: &LogPluginConfig,
    mqtt_client: &mut Connection,
    http_client: &mut JwtAuthHttpProxy,
) -> Result<(), anyhow::Error> {
    let executing = LogfileRequest::executing()?;
    let () = mqtt_client.published.send(executing).await?;

    let log_content = new_read_logs(&smartrest_request, &plugin_config)?;

    let upload_event_url = http_client
        .upload_log_binary(&smartrest_request.log_type, &log_content)
        .await?;

    let successful = LogfileRequest::successful(Some(upload_event_url))?;
    let () = mqtt_client.published.send(successful).await?;

    Ok(())
}

/// updates the log types on Cumulocity
/// sends 118,typeA,typeB,... on mqtt
pub async fn handle_dynamic_log_type_update(
    mqtt_client: &mut Connection,
    config_dir: &Path,
) -> Result<LogPluginConfig, anyhow::Error> {
    let plugin_config = LogPluginConfig::new(config_dir);
    let msg = plugin_config.to_supported_config_types_message()?;
    let () = mqtt_client.published.send(msg).await?;
    Ok(plugin_config)
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        path::{Path, PathBuf},
    };

    use c8y_smartrest::smartrest_deserializer::{SmartRestLogRequest, SmartRestRequestGeneric};
    use tempfile::TempDir;

    use crate::config::{FileEntry, LogPluginConfig};

    use super::{filter_logs_on_type, filter_logs_path_on_metadata, read_log_content};

    fn get_filter_on_logs_type() -> Result<(TempDir, Vec<PathBuf>), anyhow::Error> {
        let tempdir = TempDir::new()?;
        let tempdir_path = tempdir
            .path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("temp dir not created"))?;

        std::fs::File::create(&format!("{tempdir_path}/file_a"))?;
        std::fs::File::create(&format!("{tempdir_path}/file_b"))?;
        std::fs::File::create(&format!("{tempdir_path}/file_c"))?;

        let files = vec![
            FileEntry {
                path: format!("{tempdir_path}/file_a"),
                config_type: "type_one".to_string(),
            },
            FileEntry {
                path: format!("{tempdir_path}/file_b"),
                config_type: "type_one".to_string(),
            },
            FileEntry {
                path: format!("{tempdir_path}/file_c"),
                config_type: "type_two".to_string(),
            },
        ];
        let logs_config = LogPluginConfig { files: files };

        let smartrest_obj = SmartRestLogRequest::from_smartrest(
            "522,DeviceSerial,type_one,2021-01-01T00:00:00+0200,2021-01-10T00:00:00+0200,,1000",
        )?;

        let after_file = filter_logs_on_type(&smartrest_obj, &logs_config)?;
        Ok((tempdir, after_file))
    }

    #[test]
    fn test_filter_logs_on_type() {
        let (tempdir, after_file) = get_filter_on_logs_type().unwrap();
        let tempdir_path = tempdir.path().to_str().unwrap();
        assert_eq!(
            after_file,
            vec![
                PathBuf::from(&format!("{tempdir_path}/file_a")),
                PathBuf::from(&format!("{tempdir_path}/file_b"))
            ]
        )
    }

    #[test]
    fn test_filter_logs_path_on_metadata() {
        let smartrest_obj = SmartRestLogRequest::from_smartrest(
            "522,DeviceSerial,type_one,2021-01-01T00:00:00+0200,2021-01-10T00:00:00+0200,,1000",
        )
        .unwrap();
        let (_tempdir, logs) = get_filter_on_logs_type().unwrap();
        filter_logs_path_on_metadata(&smartrest_obj, logs).unwrap();
    }

    #[test]
    fn test_read_log_content() {
        let tempdir = TempDir::new().unwrap();
        let tempdir_path = tempdir
            .path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("temp dir not created"))
            .unwrap();
        let file_path = &format!("{tempdir_path}/file_a.log");

        let mut log_file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .write(true)
            .open(file_path)
            .unwrap();

        let data = "this is the first line.\nthis is the second line.\nthis is the third line.\nthis is the forth line.\nthis is the fifth line.";

        let () = log_file.write_all(data.as_bytes()).unwrap();

        let line_counter = 0;
        let max_lines = 4;
        let filter_text = None;

        let (line_counter, result) =
            read_log_content(Path::new(file_path), line_counter, max_lines, &filter_text).unwrap();

        assert_eq!(line_counter, max_lines);
        assert_eq!(result, "filename: file_a.log\nthis is the second line.\nthis is the third line.\nthis is the forth line.\nthis is the fifth line.\n");
    }
}
