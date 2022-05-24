use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

use easy_reader::EasyReader;
use glob::glob;
use time::OffsetDateTime;
use tracing::info;

use crate::{config::LogPluginConfig, error::LogRetrievalError};
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
) -> Result<(usize, String), LogRetrievalError> {
    if line_counter >= max_lines {
        Err(LogRetrievalError::MaxLines)
    } else {
        let mut file_content_as_vec = VecDeque::new();
        let file = std::fs::File::open(&logfile)?;
        let file_name = format!(
            "filename: {}\n",
            logfile.file_name().unwrap().to_str().unwrap() // never fails because we check file exists
        );
        let reader = EasyReader::new(file);
        match reader {
            Ok(mut reader) => {
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
            Err(_err) => Ok((line_counter, String::new())),
        }
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
            Err(_error @ LogRetrievalError::MaxLines) => {
                break;
            }
            Err(error) => {
                return Err(error.into());
            }
        };
    }

    Ok(output)
}

fn filter_logs_on_type(
    smartrest_obj: &SmartRestLogRequest,
    plugin_config: &LogPluginConfig,
) -> Result<Vec<PathBuf>, LogRetrievalError> {
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
    if files_to_send.is_empty() {
        Err(LogRetrievalError::NoLogsAvailableForType {
            log_type: smartrest_obj.log_type.to_string(),
        })
    } else {
        Ok(files_to_send)
    }
}

/// filter a vector of pathbufs according to `smartrest_obj.date_from` and `smartrest_obj.date_to`
fn filter_logs_path_on_metadata(
    smartrest_obj: &SmartRestLogRequest,
    mut logs_path_vec: Vec<PathBuf>,
) -> Result<Vec<PathBuf>, LogRetrievalError> {
    let mut out = vec![];

    logs_path_vec.sort_by_key(|pathbuf| {
        if let Ok(metadata) = std::fs::metadata(&pathbuf) {
            if let Ok(file_modified_time) = metadata.modified() {
                return OffsetDateTime::from(file_modified_time);
            }
        };
        // if the file metadata can not be read, we set the file's metadata
        // to UNIX_EPOCH (Jan 1st 1970)
        return OffsetDateTime::UNIX_EPOCH;
    });
    logs_path_vec.reverse(); // to get most recent

    for file_pathbuf in logs_path_vec {
        let metadata = std::fs::metadata(&file_pathbuf)?;
        let datetime_modified = OffsetDateTime::from(metadata.modified()?);
        if datetime_modified >= smartrest_obj.date_from {
            out.push(file_pathbuf);
        }
    }

    if out.is_empty() {
        Err(LogRetrievalError::NoLogsAvailableForType {
            log_type: smartrest_obj.log_type.to_string(),
        })
    } else {
        Ok(out)
    }
}

/// executes the log file request
///
/// - sends request executing (mqtt)
/// - uploads log content (http)
/// - sends request successful (mqtt)
async fn execute_logfile_request_operation(
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

    info!("Log request processed.");
    Ok(())
}
pub async fn handle_logfile_request_operation(
    smartrest_request: &SmartRestLogRequest,
    plugin_config: &LogPluginConfig,
    mqtt_client: &mut Connection,
    http_client: &mut JwtAuthHttpProxy,
) -> Result<(), anyhow::Error> {
    match execute_logfile_request_operation(
        smartrest_request,
        plugin_config,
        mqtt_client,
        http_client,
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(error) => {
            let error_message = format!("Handling of operation failed with {}", error);
            let failed_msg = LogfileRequest::failed(error_message)?;
            let () = mqtt_client.published.send(failed_msg).await?;
            Err(error)
        }
    }
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

    use c8y_smartrest::smartrest_deserializer::SmartRestLogRequest;
    use filetime::{set_file_mtime, FileTime};
    use tempfile::TempDir;
    use time::macros::datetime;

    use crate::{
        config::{FileEntry, LogPluginConfig},
        logfile_request::new_read_logs,
    };

    use super::{filter_logs_on_type, filter_logs_path_on_metadata, read_log_content};

    /// Preparing a temp directory containing four files, with
    /// two types { type_one, type_two }:
    ///
    ///     file_a, type_one
    ///     file_b, type_one
    ///     file_c, type_two
    ///     file_d, type_one
    ///
    /// each file has the following modified "file update" timestamp:
    ///     file_a has timestamp: 1970/01/01 00:00:02
    ///     file_b has timestamp: 1970/01/01 00:00:03
    ///     file_c has timestamp: 1970/01/01 00:00:11
    ///     file_d has timestamp: (current, not modified)
    fn prepare() -> Result<(TempDir, LogPluginConfig), anyhow::Error> {
        let tempdir = TempDir::new()?;
        let tempdir_path = tempdir
            .path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("temp dir not created"))?;

        std::fs::File::create(&format!("{tempdir_path}/file_a"))?;
        std::fs::File::create(&format!("{tempdir_path}/file_b"))?;
        std::fs::File::create(&format!("{tempdir_path}/file_c"))?;
        std::fs::File::create(&format!("{tempdir_path}/file_d"))?;

        let new_mtime = FileTime::from_unix_time(2, 0);
        set_file_mtime(&format!("{tempdir_path}/file_a"), new_mtime).unwrap();

        let new_mtime = FileTime::from_unix_time(3, 0);
        set_file_mtime(&format!("{tempdir_path}/file_b"), new_mtime).unwrap();

        let new_mtime = FileTime::from_unix_time(11, 0);
        set_file_mtime(&format!("{tempdir_path}/file_c"), new_mtime).unwrap();

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
            FileEntry {
                path: format!("{tempdir_path}/file_d"),
                config_type: "type_one".to_string(),
            },
        ];
        let logs_config = LogPluginConfig { files: files };
        Ok((tempdir, logs_config))
    }

    fn build_smartrest_log_request_object(
        log_type: String,
        needle: Option<String>,
        lines: usize,
    ) -> SmartRestLogRequest {
        SmartRestLogRequest {
            message_id: "522".to_string(),
            device: "device".to_string(),
            log_type: log_type,
            date_from: datetime!(1970-01-01 00:00:03 +00:00),
            date_to: datetime!(1970-01-01 00:00:00 +00:00), // not used
            needle: needle,
            lines: lines,
        }
    }

    #[test]
    /// Filter on type = "type_one".
    /// There are four logs created in tempdir { file_a, file_b, file_c, file_d }
    /// Of which, { file_a, file_b, file_d } are "type_one"
    fn test_filter_logs_on_type() {
        let (tempdir, logs_config) = prepare().unwrap();
        let tempdir_path = tempdir.path().to_str().unwrap();
        let smartrest_obj = build_smartrest_log_request_object("type_one".to_string(), None, 1000);
        let logs = filter_logs_on_type(&smartrest_obj, &logs_config).unwrap();
        assert_eq!(
            logs,
            vec![
                PathBuf::from(&format!("{tempdir_path}/file_a")),
                PathBuf::from(&format!("{tempdir_path}/file_b")),
                PathBuf::from(&format!("{tempdir_path}/file_d"))
            ]
        )
    }

    #[test]
    /// Out of logs filtered on type = "type_one", that is: { file_a, file_b, file_d }.
    /// Only logs filtered on metadata remain, that is { file_b, file_d }.
    ///
    /// This is because:
    ///
    /// file_a has timestamp: 1970/01/01 00:00:02
    /// file_b has timestamp: 1970/01/01 00:00:03
    /// file_d has timestamp: (current, not modified)
    ///
    /// The order of the output is { file_d, file_b }, because files are sorted from
    /// most recent to oldest
    fn test_filter_logs_path_on_metadata() {
        let (tempdir, logs_config) = prepare().unwrap();
        let smartrest_obj = build_smartrest_log_request_object("type_one".to_string(), None, 1000);
        let logs = filter_logs_on_type(&smartrest_obj, &logs_config).unwrap();
        let logs = filter_logs_path_on_metadata(&smartrest_obj, logs).unwrap();

        assert_eq!(
            logs,
            vec![
                PathBuf::from(format!("{}/file_d", tempdir.path().to_str().unwrap())),
                PathBuf::from(format!("{}/file_b", tempdir.path().to_str().unwrap())),
            ]
        )
    }

    #[test]
    /// Inserting 5 log lines in { file_a }:
    /// [
    ///     this is the first line.
    ///     this is the second line.
    ///     this is the third line.
    ///     this is the fourth line.
    ///     this is the fifth line.
    /// ]
    ///
    /// Requesting back only 4. Note that because we read the logs in reverse order, the first line
    /// should be ommited. The result sould be:
    /// [
    ///     this is the second line.
    ///     this is the third line.
    ///     this is the fourth line.
    ///     this is the fifth line.
    /// ]
    ///
    fn test_read_log_content() {
        let (tempdir, _logs_config) = prepare().unwrap();
        let path = tempdir.path().to_str().unwrap();
        let file_path = &format!("{path}/file_a");
        let mut log_file = std::fs::OpenOptions::new()
            .append(true)
            .create(false)
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
        assert_eq!(result, "filename: file_a\nthis is the second line.\nthis is the third line.\nthis is the forth line.\nthis is the fifth line.\n");
    }

    #[test]
    /// Inserting 5 lines of logs for each log file { file_a, ..., file_d }.
    /// Each line contains the text: "this is the { line_number } line of { file_name }
    /// where line_number { first, second, third, forth, fifth }
    /// where file_name { file_a, ..., file_d }
    ///
    /// Requesting logs for log_type = "type_one", that are older than:
    /// timestamp: 1970/01/01 00:00:03
    ///
    /// These are:
    /// file_b and file_d
    ///
    /// file_d is the newest file, so its logs are read first. then file_b.
    ///
    /// Because only 7 lines are requested (and each file has 5 lines), the expedted
    /// result is:
    ///
    /// - all logs from file_d (5)
    /// - last two logs from file_b (2)
    fn test_read_log_content_multiple_files() {
        let (tempdir, logs_config) = prepare().unwrap();
        let tempdir_path = tempdir.path().to_str().unwrap();

        for (file_name, m_time) in [
            ("file_a", 2),
            ("file_b", 3),
            ("file_c", 11),
            ("file_d", 100),
        ] {
            let file_path = &format!("{tempdir_path}/{file_name}");

            let mut log_file = std::fs::OpenOptions::new()
                .append(true)
                .create(false)
                .write(true)
                .open(file_path)
                .unwrap();

            let data = &format!("this is the first line of {file_name}.\nthis is the second line of {file_name}.\nthis is the third line of {file_name}.\nthis is the forth line of {file_name}.\nthis is the fifth line of {file_name}.");

            let () = log_file.write_all(data.as_bytes()).unwrap();

            let new_mtime = FileTime::from_unix_time(m_time, 0);
            set_file_mtime(file_path, new_mtime).unwrap();
        }

        let smartrest_obj = build_smartrest_log_request_object("type_one".to_string(), None, 7);

        let result = new_read_logs(&smartrest_obj, &logs_config).unwrap();
        assert_eq!(result, String::from("filename: file_d\nthis is the first line of file_d.\nthis is the second line of file_d.\nthis is the third line of file_d.\nthis is the forth line of file_d.\nthis is the fifth line of file_d.\nfilename: file_b\nthis is the forth line of file_b.\nthis is the fifth line of file_b.\n"))
    }
}
