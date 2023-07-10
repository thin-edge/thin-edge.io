use async_trait::async_trait;
use c8y_api::smartrest::message::get_smartrest_device_id;
use c8y_api::smartrest::smartrest_deserializer::SmartRestLogRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRestSerializer;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToExecuting;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToFailed;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToSuccessful;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_api::utils::bridge::is_c8y_bridge_up;
use c8y_api::OffsetDateTime;
use c8y_http_proxy::handle::C8YHttpProxy;
use easy_reader::EasyReader;
use glob::glob;
use log::error;
use log::info;
use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::NoMessage;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_utils::paths::PathsError;

use super::error::LogRetrievalError;
use super::LogManagerConfig;
use super::LogPluginConfig;

fan_in_message_type!(LogInput[MqttMessage, FsWatchEvent] : Debug);
fan_in_message_type!(LogOutput[MqttMessage]: Debug);

pub struct LogManagerActor {
    config: LogManagerConfig,
    plugin_config: LogPluginConfig,
    mqtt_publisher: LoggingSender<MqttMessage>,
    http_proxy: C8YHttpProxy,
    messages: SimpleMessageBox<LogInput, NoMessage>,
}

impl LogManagerActor {
    pub fn new(
        config: LogManagerConfig,
        plugin_config: LogPluginConfig,
        mqtt_publisher: LoggingSender<MqttMessage>,
        http_proxy: C8YHttpProxy,
        messages: SimpleMessageBox<LogInput, NoMessage>,
    ) -> Self {
        Self {
            config,
            plugin_config,
            mqtt_publisher,
            http_proxy,
            messages,
        }
    }

    pub async fn process_mqtt_message(
        &mut self,
        message: MqttMessage,
    ) -> Result<(), anyhow::Error> {
        if is_c8y_bridge_up(&message) {
            self.reload_supported_log_types().await?;
            self.get_pending_operations_from_cloud().await?;
        } else if let Ok(payload) = message.payload_str() {
            for smartrest_message in payload.split('\n') {
                let result = match smartrest_message.split(',').next().unwrap_or_default() {
                    "522" => {
                        info!("Log request received: {payload}");
                        match get_smartrest_device_id(payload) {
                            Some(device_id) if device_id == self.config.device_id => {
                                // retrieve smartrest object from payload
                                let maybe_smartrest_obj =
                                    SmartRestLogRequest::from_smartrest(smartrest_message);
                                if let Ok(smartrest_obj) = maybe_smartrest_obj {
                                    self.handle_logfile_request_operation(&smartrest_obj).await
                                } else {
                                    error!("Incorrect SmartREST payload: {}", smartrest_message);
                                    Ok(())
                                }
                            }
                            // Ignore operation messages created for child devices
                            _ => Ok(()),
                        }
                    }
                    _ => {
                        // Ignore operation messages not meant for this plugin
                        Ok(())
                    }
                };

                if let Err(err) = result {
                    let error_message = format!(
                        "Handling of operation: '{}' failed with {}",
                        smartrest_message, err
                    );
                    error!("{}", error_message);
                }
            }
        }
        Ok(())
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
            let file = std::fs::File::open(logfile)?;
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

    /// read any log file coming from `smartrest_obj.log_type`
    pub fn new_read_logs(
        &mut self,
        smartrest_obj: &SmartRestLogRequest,
    ) -> Result<String, anyhow::Error> {
        let mut output = String::new();
        // first filter logs on type
        let mut logfiles_to_read = self.filter_logs_on_type(smartrest_obj)?;
        logfiles_to_read = Self::filter_logs_path_on_metadata(smartrest_obj, logfiles_to_read)?;

        let mut line_counter = 0usize;
        for logfile in logfiles_to_read {
            match Self::read_log_content(
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
        &mut self,
        smartrest_obj: &SmartRestLogRequest,
    ) -> Result<Vec<PathBuf>, LogRetrievalError> {
        let mut files_to_send = Vec::new();
        for files in &self.plugin_config.files {
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
            if let Ok(metadata) = std::fs::metadata(pathbuf) {
                if let Ok(file_modified_time) = metadata.modified() {
                    return OffsetDateTime::from(file_modified_time);
                }
            };
            // if the file metadata can not be read, we set the file's metadata
            // to UNIX_EPOCH (Jan 1st 1970)
            OffsetDateTime::UNIX_EPOCH
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
        &mut self,
        smartrest_request: &SmartRestLogRequest,
    ) -> Result<(), anyhow::Error> {
        let executing = LogfileRequest::executing()?;
        self.mqtt_publisher.send(executing).await?;

        let log_content = self.new_read_logs(smartrest_request)?;

        let upload_event_url = self
            .http_proxy
            .upload_log_binary(&smartrest_request.log_type, &log_content, None)
            .await?;

        let successful = LogfileRequest::successful(Some(upload_event_url))?;
        self.mqtt_publisher.send(successful).await?;

        info!("Log request processed.");
        Ok(())
    }
    pub async fn handle_logfile_request_operation(
        &mut self,
        smartrest_request: &SmartRestLogRequest,
    ) -> Result<(), anyhow::Error> {
        match self
            .execute_logfile_request_operation(smartrest_request)
            .await
        {
            Ok(()) => Ok(()),
            Err(error) => {
                let error_message = format!("Handling of operation failed with {}", error);
                let failed_msg = LogfileRequest::failed(error_message)?;
                self.mqtt_publisher.send(failed_msg).await?;
                Err(error)
            }
        }
    }

    pub async fn process_file_watch_events(
        &mut self,
        event: FsWatchEvent,
    ) -> Result<(), anyhow::Error> {
        let path = match event {
            FsWatchEvent::Modified(path) => path,
            FsWatchEvent::FileDeleted(path) => path,
            FsWatchEvent::FileCreated(path) => path,
            FsWatchEvent::DirectoryDeleted(_) => return Ok(()),
            FsWatchEvent::DirectoryCreated(_) => return Ok(()),
        };

        if path
            .file_name()
            .ok_or_else(|| PathsError::ParentDirNotFound {
                path: path.as_os_str().into(),
            })?
            .eq("c8y-log-plugin.toml")
        {
            self.reload_supported_log_types().await?;
        }

        Ok(())
    }

    pub async fn reload_supported_log_types(&mut self) -> Result<(), anyhow::Error> {
        let plugin_config = LogPluginConfig::new(self.config.plugin_config_path.as_path());
        self.publish_supported_log_types(&plugin_config).await
    }

    /// updates the log types on Cumulocity
    /// sends 118,typeA,typeB,... on mqtt
    pub async fn publish_supported_log_types(
        &mut self,
        plugin_config: &LogPluginConfig,
    ) -> Result<(), anyhow::Error> {
        let msg = plugin_config.to_supported_config_types_message()?;
        Ok(self.mqtt_publisher.send(msg).await?)
    }

    async fn get_pending_operations_from_cloud(&mut self) -> Result<(), anyhow::Error> {
        // Get pending operations
        let msg = MqttMessage::new(&C8yTopic::SmartRestResponse.to_topic()?, "500");
        self.mqtt_publisher.send(msg).await?;
        Ok(())
    }
}

#[async_trait]
impl Actor for LogManagerActor {
    fn name(&self) -> &str {
        "LogManager"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        self.reload_supported_log_types().await.unwrap();
        self.get_pending_operations_from_cloud().await.unwrap();

        while let Some(event) = self.messages.recv().await {
            match event {
                LogInput::MqttMessage(message) => {
                    self.process_mqtt_message(message).await.unwrap();
                }
                LogInput::FsWatchEvent(event) => {
                    self.process_file_watch_events(event).await.unwrap();
                }
            }
        }
        Ok(())
    }
}

pub struct LogfileRequest {}

impl TryIntoOperationStatusMessage for LogfileRequest {
    /// returns a c8y message specifying to set log status to executing.
    ///
    /// example message: '501,c8y_LogfileRequest'
    fn status_executing() -> Result<
        c8y_api::smartrest::smartrest_serializer::SmartRest,
        c8y_api::smartrest::error::SmartRestSerializerError,
    > {
        SmartRestSetOperationToExecuting::new(CumulocitySupportedOperations::C8yLogFileRequest)
            .to_smartrest()
    }

    fn status_successful(
        parameter: Option<String>,
    ) -> Result<
        c8y_api::smartrest::smartrest_serializer::SmartRest,
        c8y_api::smartrest::error::SmartRestSerializerError,
    > {
        SmartRestSetOperationToSuccessful::new(CumulocitySupportedOperations::C8yLogFileRequest)
            .with_response_parameter(&parameter.unwrap())
            .to_smartrest()
    }

    fn status_failed(
        failure_reason: String,
    ) -> Result<
        c8y_api::smartrest::smartrest_serializer::SmartRest,
        c8y_api::smartrest::error::SmartRestSerializerError,
    > {
        SmartRestSetOperationToFailed::new(
            CumulocitySupportedOperations::C8yLogFileRequest,
            failure_reason,
        )
        .to_smartrest()
    }
}

#[cfg(test)]
mod tests {
    use super::LogManagerActor;
    use crate::LogManagerBuilder;
    use crate::LogManagerConfig;
    use crate::Topic;
    use c8y_api::smartrest::smartrest_deserializer::SmartRestLogRequest;
    use c8y_http_proxy::messages::C8YRestRequest;
    use c8y_http_proxy::messages::C8YRestResponse;
    use c8y_http_proxy::messages::C8YRestResult;
    use c8y_http_proxy::messages::UploadLogBinary;
    use filetime::set_file_mtime;
    use filetime::FileTime;
    use std::io::Write;
    use std::net::Ipv4Addr;
    use std::path::Path;
    use std::path::PathBuf;
    use tedge_actors::Actor;
    use tedge_actors::Builder;
    use tedge_actors::MessageReceiver;
    use tedge_actors::NoMessage;
    use tedge_actors::Sender;
    use tedge_actors::SimpleMessageBox;
    use tedge_actors::SimpleMessageBoxBuilder;
    use tedge_file_system_ext::FsWatchEvent;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_test_utils::fs::TempTedgeDir;
    use time::macros::datetime;

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
    fn prepare() -> Result<TempTedgeDir, anyhow::Error> {
        let tempdir = TempTedgeDir::new();
        let tempdir_path = tempdir
            .path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("temp dir not created"))?;

        std::fs::File::create(format!("{tempdir_path}/file_a"))?;
        std::fs::File::create(format!("{tempdir_path}/file_b"))?;
        tempdir.file("file_c").with_raw_content("Some content");
        std::fs::File::create(format!("{tempdir_path}/file_d"))?;

        let new_mtime = FileTime::from_unix_time(2, 0);
        set_file_mtime(format!("{tempdir_path}/file_a"), new_mtime).unwrap();

        let new_mtime = FileTime::from_unix_time(3, 0);
        set_file_mtime(format!("{tempdir_path}/file_b"), new_mtime).unwrap();

        let new_mtime = FileTime::from_unix_time(11, 0);
        set_file_mtime(format!("{tempdir_path}/file_c"), new_mtime).unwrap();

        tempdir
            .file("c8y-log-plugin.toml")
            .with_raw_content(&format!(
                r#"files = [
            {{ type = "type_one", path = "{tempdir_path}/file_a" }},
            {{ type = "type_one", path = "{tempdir_path}/file_b" }},
            {{ type = "type_two", path = "{tempdir_path}/file_c" }},
            {{ type = "type_one", path = "{tempdir_path}/file_d" }},
        ]"#
            ));

        Ok(tempdir)
    }

    fn build_smartrest_log_request_object(
        log_type: String,
        needle: Option<String>,
        lines: usize,
    ) -> SmartRestLogRequest {
        SmartRestLogRequest {
            message_id: "522".to_string(),
            device: "device".to_string(),
            log_type,
            date_from: datetime!(1970-01-01 00:00:03 +00:00),
            date_to: datetime!(1970-01-01 00:00:00 +00:00), // not used
            needle,
            lines,
        }
    }

    /// Create a log manager actor builder
    /// along two boxes to exchange MQTT and HTTP messages with the log actor
    #[allow(clippy::type_complexity)]
    fn new_log_manager_builder(
        temp_dir: &Path,
    ) -> (
        LogManagerBuilder,
        SimpleMessageBox<MqttMessage, MqttMessage>,
        SimpleMessageBox<C8YRestRequest, C8YRestResult>,
        SimpleMessageBox<NoMessage, FsWatchEvent>,
    ) {
        let config = LogManagerConfig {
            config_dir: temp_dir.to_path_buf(),
            log_dir: temp_dir.to_path_buf(),
            tmp_dir: temp_dir.to_path_buf(),
            device_id: "SUT".to_string(),
            mqtt_host: "127.0.0.1".to_string(),
            mqtt_port: 1883,
            tedge_http_host: Ipv4Addr::LOCALHOST.into(),
            tedge_http_port: 80,
            ops_dir: temp_dir.to_path_buf(),
            plugin_config_dir: temp_dir.to_path_buf(),
            plugin_config_path: temp_dir.join("c8y-log-plugin.toml"),
        };

        let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
            SimpleMessageBoxBuilder::new("MQTT", 5);
        let mut c8y_proxy_builder: SimpleMessageBoxBuilder<C8YRestRequest, C8YRestResult> =
            SimpleMessageBoxBuilder::new("C8Y", 1);
        let mut fs_watcher_builder: SimpleMessageBoxBuilder<NoMessage, FsWatchEvent> =
            SimpleMessageBoxBuilder::new("FS", 5);

        let log_builder = LogManagerBuilder::try_new(
            config,
            &mut mqtt_builder,
            &mut c8y_proxy_builder,
            &mut fs_watcher_builder,
        )
        .unwrap();

        (
            log_builder,
            mqtt_builder.build(),
            c8y_proxy_builder.build(),
            fs_watcher_builder.build(),
        )
    }

    /// Create a log manager actor ready for testing
    fn new_log_manager_actor(temp_dir: &Path) -> LogManagerActor {
        let (actor_builder, _, _, _) = new_log_manager_builder(temp_dir);
        actor_builder.build()
    }

    /// Spawn a log manager actor and return 2 boxes to exchange MQTT and HTTP messages with it
    fn spawn_log_manager_actor(
        temp_dir: &Path,
    ) -> (
        SimpleMessageBox<MqttMessage, MqttMessage>,
        SimpleMessageBox<C8YRestRequest, C8YRestResult>,
        SimpleMessageBox<NoMessage, FsWatchEvent>,
    ) {
        let (actor_builder, mqtt, http, fs) = new_log_manager_builder(temp_dir);
        let mut actor = actor_builder.build();
        tokio::spawn(async move { actor.run().await });
        (mqtt, http, fs)
    }

    #[test]
    /// Filter on type = "type_one".
    /// There are four logs created in tempdir { file_a, file_b, file_c, file_d }
    /// Of which, { file_a, file_b, file_d } are "type_one"
    fn test_filter_logs_on_type() {
        let tempdir = prepare().unwrap();
        let tempdir_path = tempdir.path().to_str().unwrap();
        let smartrest_obj = build_smartrest_log_request_object("type_one".to_string(), None, 1000);
        let mut actor = new_log_manager_actor(tempdir.path());

        let logs = actor.filter_logs_on_type(&smartrest_obj).unwrap();
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
        let tempdir = prepare().unwrap();
        let smartrest_obj = build_smartrest_log_request_object("type_one".to_string(), None, 1000);
        let mut actor = new_log_manager_actor(tempdir.path());

        let logs = actor.filter_logs_on_type(&smartrest_obj).unwrap();
        let logs = LogManagerActor::filter_logs_path_on_metadata(&smartrest_obj, logs).unwrap();

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
    /// should be omitted. The result should be:
    /// [
    ///     this is the second line.
    ///     this is the third line.
    ///     this is the fourth line.
    ///     this is the fifth line.
    /// ]
    ///
    fn test_read_log_content() {
        let tempdir = prepare().unwrap();
        let path = tempdir.path().to_str().unwrap();
        let file_path = &format!("{path}/file_a");
        let mut log_file = std::fs::OpenOptions::new()
            .append(true)
            .create(false)
            .write(true)
            .open(file_path)
            .unwrap();

        let data = "this is the first line.\nthis is the second line.\nthis is the third line.\nthis is the forth line.\nthis is the fifth line.";

        log_file.write_all(data.as_bytes()).unwrap();

        let line_counter = 0;
        let max_lines = 4;
        let filter_text = None;

        let (line_counter, result) = LogManagerActor::read_log_content(
            Path::new(file_path),
            line_counter,
            max_lines,
            &filter_text,
        )
        .unwrap();

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
        let tempdir = prepare().unwrap();
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

            log_file.write_all(data.as_bytes()).unwrap();

            let new_mtime = FileTime::from_unix_time(m_time, 0);
            set_file_mtime(file_path, new_mtime).unwrap();
        }

        let smartrest_obj = build_smartrest_log_request_object("type_one".to_string(), None, 7);
        let mut actor = new_log_manager_actor(tempdir.path());

        let result = actor.new_read_logs(&smartrest_obj).unwrap();
        assert_eq!(result, String::from("filename: file_d\nthis is the first line of file_d.\nthis is the second line of file_d.\nthis is the third line of file_d.\nthis is the forth line of file_d.\nthis is the fifth line of file_d.\nfilename: file_b\nthis is the forth line of file_b.\nthis is the fifth line of file_b.\n"))
    }

    #[tokio::test]
    async fn log_manager_send_log_types_on_start_and_bridge_up_and_config_update(
    ) -> Result<(), anyhow::Error> {
        let tempdir = prepare()?;
        let (mut mqtt, _http, mut fs) = spawn_log_manager_actor(tempdir.path());

        let c8y_s_us = Topic::new_unchecked("c8y/s/us");
        let bridge = Topic::new_unchecked("tedge/health/mosquitto-c8y-bridge");

        assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(&c8y_s_us, "118,type_one,type_two"))
        );
        assert_eq!(mqtt.recv().await, Some(MqttMessage::new(&c8y_s_us, "500")));

        mqtt.send(MqttMessage::new(&bridge, "1")).await?;
        assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(&c8y_s_us, "118,type_one,type_two"))
        );
        assert_eq!(mqtt.recv().await, Some(MqttMessage::new(&c8y_s_us, "500")));

        fs.send(FsWatchEvent::Modified(
            tempdir.path().join("c8y-log-plugin.toml"),
        ))
        .await?;
        assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(&c8y_s_us, "118,type_one,type_two"))
        );

        Ok(())
    }

    #[tokio::test]
    async fn log_manager_upload_log_files_on_request() -> Result<(), anyhow::Error> {
        let tempdir = prepare()?;
        let (mut mqtt, mut http, _fs) = spawn_log_manager_actor(tempdir.path());

        let c8y_s_ds = Topic::new_unchecked("c8y/s/ds");
        let c8y_s_us = Topic::new_unchecked("c8y/s/us");

        // Let's ignore the 2 init messages sent on start
        assert!(mqtt.recv().await.is_some());
        assert!(mqtt.recv().await.is_some());

        // When a log request is received
        let log_request =
            "522,SUT,type_two,1970-01-01T00:00:00+0000,1970-01-01T00:00:30+0000,,1000";
        mqtt.send(MqttMessage::new(&c8y_s_ds, log_request)).await?;

        // The log manager notifies C8Y that the request has been received and is processed
        assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(&c8y_s_us, "501,c8y_LogfileRequest\n"))
        );

        // Then uploads the requested content over HTTP
        assert_eq!(
            http.recv().await,
            Some(C8YRestRequest::UploadLogBinary(UploadLogBinary {
                log_type: "type_two".to_string(),
                log_content: "filename: file_c\nSome content\n".to_string(),
                child_device_id: None
            }))
        );

        // C8Y responds with an event id
        http.send(Ok(C8YRestResponse::EventId("12345".to_string())))
            .await?;

        // Finally, the log manager uses the event id to notify C8Y that the request has been fully processed
        assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &c8y_s_us,
                "503,c8y_LogfileRequest,12345\n"
            ))
        );

        Ok(())
    }
}
