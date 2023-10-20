use super::LogManagerConfig;
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
use c8y_http_proxy::handle::C8YHttpProxy;
use log::error;
use log::info;
use log::trace;
use log_manager::LogPluginConfig;
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
                        let topic = &message.topic.name;
                        info!("Log request received on topic: {topic}");
                        trace!("payload: {payload}");
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

        let log_path = log_manager::new_read_logs(
            &self.plugin_config.files,
            &smartrest_request.log_type,
            smartrest_request.date_from,
            smartrest_request.lines,
            &smartrest_request.search_text,
        )?;

        let log_content = std::fs::read_to_string(&log_path)?;

        let upload_event_url = self
            .http_proxy
            .upload_log_binary(
                &smartrest_request.log_type,
                &log_content,
                self.config.device_id.clone(),
            )
            .await?;

        let successful = LogfileRequest::successful(Some(upload_event_url))?;
        self.mqtt_publisher.send(successful).await?;

        std::fs::remove_file(log_path)?;

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
                error!(
                    "Handling of operation for log type {} failed with: {}",
                    smartrest_request.log_type, error
                );
                Ok(())
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
        self.plugin_config = LogPluginConfig::new(self.config.plugin_config_path.as_path());
        self.publish_supported_log_types().await
    }

    /// updates the log types on Cumulocity
    /// sends 118,typeA,typeB,... on mqtt
    pub async fn publish_supported_log_types(&mut self) -> Result<(), anyhow::Error> {
        let topic = C8yTopic::SmartRestResponse.to_topic()?;
        let mut config_types = self.plugin_config.get_all_file_types();
        config_types.sort();
        let supported_config_types = config_types.join(",");
        let payload = format!("118,{supported_config_types}");
        let msg = MqttMessage::new(&topic, payload);
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

    async fn run(mut self) -> Result<(), RuntimeError> {
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
    use crate::LogManagerBuilder;
    use crate::LogManagerConfig;
    use crate::Topic;
    use c8y_http_proxy::messages::C8YRestRequest;
    use c8y_http_proxy::messages::C8YRestResponse;
    use c8y_http_proxy::messages::C8YRestResult;
    use c8y_http_proxy::messages::UploadLogBinary;
    use filetime::set_file_mtime;
    use filetime::FileTime;
    use std::net::Ipv4Addr;
    use std::path::Path;
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

    /// Spawn a log manager actor and return 2 boxes to exchange MQTT and HTTP messages with it
    fn spawn_log_manager_actor(
        temp_dir: &Path,
    ) -> (
        SimpleMessageBox<MqttMessage, MqttMessage>,
        SimpleMessageBox<C8YRestRequest, C8YRestResult>,
        SimpleMessageBox<NoMessage, FsWatchEvent>,
    ) {
        let (actor_builder, mqtt, http, fs) = new_log_manager_builder(temp_dir);
        let actor = actor_builder.build();
        tokio::spawn(async move { actor.run().await });
        (mqtt, http, fs)
    }

    #[tokio::test]
    async fn log_manager_send_log_types_on_start_and_bridge_up_and_config_update(
    ) -> Result<(), anyhow::Error> {
        let tempdir = prepare()?;
        let (mut mqtt, _http, mut fs) = spawn_log_manager_actor(tempdir.path());

        let c8y_s_us = Topic::new_unchecked("c8y/s/us");
        let bridge =
            Topic::new_unchecked("te/device/main/service/mosquitto-c8y-bridge/status/health");

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
                device_id: "SUT".into()
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
