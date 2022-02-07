use crate::{
    component::TEdgeComponent,
    operations::Operations,
    sm_c8y_mapper::{
        error::*,
        http_proxy::{C8YHttpProxy, JwtAuthHttpProxy},
        json_c8y::C8yUpdateSoftwareListResponse,
        topic::*,
    },
};

use agent_interface::{
    topic::*, Jsonify, OperationStatus, RestartOperationRequest, RestartOperationResponse,
    SoftwareListRequest, SoftwareListResponse, SoftwareUpdateResponse,
};
use async_trait::async_trait;
use c8y_smartrest::smartrest_deserializer::{SmartRestLogRequest, SmartRestRestartRequest};
use c8y_smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_smartrest::{
    error::SmartRestDeserializerError,
    smartrest_deserializer::SmartRestUpdateSoftware,
    smartrest_serializer::{
        SmartRestGetPendingOperations, SmartRestSerializer, SmartRestSetOperationToExecuting,
        SmartRestSetOperationToFailed, SmartRestSetOperationToSuccessful,
        SmartRestSetSupportedLogType,
    },
};
use chrono::{DateTime, FixedOffset};
use download::{Auth, DownloadInfo};
use mqtt_channel::{Config, Connection, MqttError, SinkExt, StreamExt, Topic, TopicFilter};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use std::{convert::TryInto, process::Stdio};
use tedge_config::{ConfigSettingAccessor, MqttPortSetting, TEdgeConfig};
use tracing::{debug, error, info, instrument};

const AGENT_LOG_DIR: &str = "/var/log/tedge/agent";
const SM_MAPPER: &str = "SM-C8Y-Mapper";

pub struct CumulocitySoftwareManagementMapper {}

impl CumulocitySoftwareManagementMapper {
    pub fn new() -> Self {
        Self {}
    }

    pub fn subscriptions(operations: &Operations) -> Result<TopicFilter, anyhow::Error> {
        let mut topic_filter = TopicFilter::new(ResponseTopic::SoftwareListResponse.as_str())?;
        topic_filter.add(ResponseTopic::SoftwareUpdateResponse.as_str())?;
        topic_filter.add(C8yTopic::SmartRestRequest.as_str())?;
        topic_filter.add(ResponseTopic::RestartResponse.as_str())?;

        for topic in operations.topics_for_operations() {
            topic_filter.add(&topic)?
        }

        Ok(topic_filter)
    }

    pub async fn init_session(&mut self) -> Result<(), anyhow::Error> {
        info!("Initialize tedge sm mapper session");
        let operations = Operations::try_new("/etc/tedge/operations", "c8y")?;
        let mqtt_topic = CumulocitySoftwareManagementMapper::subscriptions(&operations)?;
        let config = Config::default()
            .with_session_name(SM_MAPPER)
            .with_clean_session(false)
            .with_subscriptions(mqtt_topic);
        mqtt_channel::init_session(&config).await?;
        Ok(())
    }

    pub async fn clear_session(&mut self) -> Result<(), anyhow::Error> {
        info!("Clear tedge sm mapper session");
        let operations = Operations::try_new("/etc/tedge/operations", "c8y")?;
        let mqtt_topic = CumulocitySoftwareManagementMapper::subscriptions(&operations)?;
        let config = Config::default()
            .with_session_name(SM_MAPPER)
            .with_clean_session(true)
            .with_subscriptions(mqtt_topic);
        mqtt_channel::clear_session(&config).await?;
        Ok(())
    }
}

#[async_trait]
impl TEdgeComponent for CumulocitySoftwareManagementMapper {
    #[instrument(skip(self, tedge_config), name = "sm-c8y-mapper")]
    async fn start(&self, tedge_config: TEdgeConfig) -> Result<(), anyhow::Error> {
        let operations = Operations::try_new("/etc/tedge/operations", "c8y")?;
        let http_proxy = JwtAuthHttpProxy::try_new(&tedge_config).await?;
        let mut sm_mapper =
            CumulocitySoftwareManagement::try_new(&tedge_config, http_proxy, operations).await?;

        let () = sm_mapper.run().await?;

        Ok(())
    }
}

pub struct CumulocitySoftwareManagement<Proxy>
where
    Proxy: C8YHttpProxy,
{
    client: Connection,
    http_proxy: Proxy,
    operations: Operations,
}

impl<Proxy> CumulocitySoftwareManagement<Proxy>
where
    Proxy: C8YHttpProxy,
{
    pub fn new(client: Connection, http_proxy: Proxy, operations: Operations) -> Self {
        Self {
            client,
            http_proxy,
            operations,
        }
    }

    pub async fn try_new(
        tedge_config: &TEdgeConfig,
        http_proxy: Proxy,
        operations: Operations,
    ) -> Result<Self, anyhow::Error> {
        let mqtt_topic = CumulocitySoftwareManagementMapper::subscriptions(&operations)?;
        let mqtt_port = tedge_config.query(MqttPortSetting)?.into();

        let mqtt_config = crate::mapper::mqtt_config(SM_MAPPER, mqtt_port, mqtt_topic)?;
        let client = Connection::new(&mqtt_config).await?;

        Ok(Self {
            client,
            http_proxy,
            operations,
        })
    }

    pub async fn run(&mut self) -> Result<(), anyhow::Error> {
        info!("Initialisation");
        let () = self.http_proxy.init().await?;

        info!("Running");
        let () = self.publish_supported_log_types().await?;
        let () = self.publish_get_pending_operations().await?;
        let () = self.ask_software_list().await?;

        while let Err(err) = self.subscribe_messages_runtime().await {
            if let SMCumulocityMapperError::FromSmartRestDeserializer(
                SmartRestDeserializerError::InvalidParameter { operation, .. },
            ) = &err
            {
                let topic = C8yTopic::SmartRestResponse.to_topic()?;
                // publish the operation status as `executing`
                let () = self.publish(&topic, format!("501,{}", operation)).await?;
                // publish the operation status as `failed`
                let () = self
                    .publish(
                        &topic,
                        format!("502,{},\"{}\"", operation, &err.to_string()),
                    )
                    .await?;
            }
            error!("{}", err);
        }

        Ok(())
    }

    async fn process_smartrest(&mut self, payload: &str) -> Result<(), SMCumulocityMapperError> {
        let message_id: &str = &payload[..3];
        match message_id {
            "528" => {
                let () = self.forward_software_request(payload).await?;
            }
            "522" => {
                let () = self.forward_log_request(payload).await?;
            }
            "510" => {
                let () = self.forward_restart_request(payload).await?;
            }
            template => match self.operations.matching_smartrest_template(template) {
                Some(operation) => {
                    if let Some(command) = operation.command() {
                        execute_operation(payload, command.as_str()).await?;
                    }
                }
                None => {
                    return Err(SMCumulocityMapperError::UnknownOperation(
                        template.to_string(),
                    ));
                }
            },
        }

        Ok(())
    }

    #[instrument(skip(self), name = "main-loop")]
    async fn subscribe_messages_runtime(&mut self) -> Result<(), SMCumulocityMapperError> {
        while let Some(message) = self.client.received.next().await {
            let request_topic = message.topic.clone().try_into()?;
            match request_topic {
                MapperSubscribeTopic::ResponseTopic(ResponseTopic::SoftwareListResponse) => {
                    debug!("Software list");
                    let () = self
                        .validate_and_publish_software_list(message.payload_str()?)
                        .await?;
                }
                MapperSubscribeTopic::ResponseTopic(ResponseTopic::SoftwareUpdateResponse) => {
                    debug!("Software update");
                    let () = self
                        .publish_operation_status(message.payload_str()?)
                        .await?;
                }
                MapperSubscribeTopic::ResponseTopic(ResponseTopic::RestartResponse) => {
                    let () = self
                        .publish_restart_operation_status(message.payload_str()?)
                        .await?;
                }
                MapperSubscribeTopic::C8yTopic(_) => {
                    debug!("Cumulocity");
                    let () = self.process_smartrest(message.payload_str()?).await?;
                }
            }
        }
        Ok(())
    }

    #[instrument(skip(self), name = "software-list")]
    async fn ask_software_list(&mut self) -> Result<(), SMCumulocityMapperError> {
        let request = SoftwareListRequest::new();
        let topic = Topic::new(RequestTopic::SoftwareListRequest.as_str())?;
        let json_list_request = request.to_json()?;
        let () = self.publish(&topic, json_list_request).await?;

        Ok(())
    }

    #[instrument(skip(self), name = "software-update")]
    async fn validate_and_publish_software_list(
        &mut self,
        json_response: &str,
    ) -> Result<(), SMCumulocityMapperError> {
        let response = SoftwareListResponse::from_json(json_response)?;

        match response.status() {
            OperationStatus::Successful => {
                let () = self.send_software_list_http(&response).await?;
            }

            OperationStatus::Failed => {
                error!("Received a failed software response: {}", json_response);
            }

            OperationStatus::Executing => {} // C8Y doesn't expect any message to be published
        }

        Ok(())
    }

    async fn publish_supported_log_types(&mut self) -> Result<(), SMCumulocityMapperError> {
        let payload = SmartRestSetSupportedLogType::default().to_smartrest()?;
        let topic = C8yTopic::SmartRestResponse.to_topic()?;
        let () = self.publish(&topic, payload).await?;
        Ok(())
    }

    async fn publish_get_pending_operations(&mut self) -> Result<(), SMCumulocityMapperError> {
        let data = SmartRestGetPendingOperations::default();
        let topic = C8yTopic::SmartRestResponse.to_topic()?;
        let payload = data.to_smartrest()?;
        let () = self.publish(&topic, payload).await?;
        Ok(())
    }

    async fn publish_operation_status(
        &mut self,
        json_response: &str,
    ) -> Result<(), SMCumulocityMapperError> {
        let response = SoftwareUpdateResponse::from_json(json_response)?;
        let topic = C8yTopic::SmartRestResponse.to_topic()?;
        match response.status() {
            OperationStatus::Executing => {
                let smartrest_set_operation_status =
                    SmartRestSetOperationToExecuting::from_thin_edge_json(response)?
                        .to_smartrest()?;
                let () = self.publish(&topic, smartrest_set_operation_status).await?;
            }
            OperationStatus::Successful => {
                let smartrest_set_operation =
                    SmartRestSetOperationToSuccessful::from_thin_edge_json(response)?
                        .to_smartrest()?;
                let () = self.publish(&topic, smartrest_set_operation).await?;
                let () = self
                    .validate_and_publish_software_list(json_response)
                    .await?;
            }
            OperationStatus::Failed => {
                let smartrest_set_operation =
                    SmartRestSetOperationToFailed::from_thin_edge_json(response)?.to_smartrest()?;
                let () = self.publish(&topic, smartrest_set_operation).await?;
                let () = self
                    .validate_and_publish_software_list(json_response)
                    .await?;
            }
        };
        Ok(())
    }

    async fn publish_restart_operation_status(
        &mut self,
        json_response: &str,
    ) -> Result<(), SMCumulocityMapperError> {
        let response = RestartOperationResponse::from_json(json_response)?;
        let topic = C8yTopic::SmartRestResponse.to_topic()?;

        match response.status() {
            OperationStatus::Executing => {
                let smartrest_set_operation = SmartRestSetOperationToExecuting::new(
                    CumulocitySupportedOperations::C8yRestartRequest,
                )
                .to_smartrest()?;

                let () = self.publish(&topic, smartrest_set_operation).await?;
            }
            OperationStatus::Successful => {
                let smartrest_set_operation = SmartRestSetOperationToSuccessful::new(
                    CumulocitySupportedOperations::C8yRestartRequest,
                )
                .to_smartrest()?;
                let () = self.publish(&topic, smartrest_set_operation).await?;
            }
            OperationStatus::Failed => {
                let smartrest_set_operation = SmartRestSetOperationToFailed::new(
                    CumulocitySupportedOperations::C8yRestartRequest,
                    "Restart Failed".into(),
                )
                .to_smartrest()?;
                let () = self.publish(&topic, smartrest_set_operation).await?;
            }
        }
        Ok(())
    }

    async fn set_log_file_request_executing(&mut self) -> Result<(), SMCumulocityMapperError> {
        let topic = C8yTopic::SmartRestResponse.to_topic()?;
        let smartrest_set_operation_status =
            SmartRestSetOperationToExecuting::new(CumulocitySupportedOperations::C8yLogFileRequest)
                .to_smartrest()?;

        let () = self.publish(&topic, smartrest_set_operation_status).await?;
        Ok(())
    }

    async fn set_log_file_request_done(
        &mut self,
        binary_upload_event_url: &str,
    ) -> Result<(), SMCumulocityMapperError> {
        let topic = C8yTopic::SmartRestResponse.to_topic()?;
        let smartrest_set_operation_status = SmartRestSetOperationToSuccessful::new(
            CumulocitySupportedOperations::C8yLogFileRequest,
        )
        .with_response_parameter(binary_upload_event_url)
        .to_smartrest()?;

        let () = self.publish(&topic, smartrest_set_operation_status).await?;
        Ok(())
    }

    async fn forward_log_request(&mut self, payload: &str) -> Result<(), SMCumulocityMapperError> {
        // retrieve smartrest object from payload
        let smartrest_obj = SmartRestLogRequest::from_smartrest(&payload)?;

        // 1. set log file request to executing
        let () = self.set_log_file_request_executing().await?;

        // 2. read logs
        let log_output = read_tedge_logs(&smartrest_obj, AGENT_LOG_DIR)?;

        // 3. upload log file
        let binary_upload_event_url = self
            .http_proxy
            .upload_log_binary(log_output.as_str())
            .await?;

        // 4. set log file request to done
        let () = self
            .set_log_file_request_done(&binary_upload_event_url)
            .await?;

        info!("Log file request uploaded");

        Ok(())
    }

    async fn forward_software_request(
        &mut self,
        smartrest: &str,
    ) -> Result<(), SMCumulocityMapperError> {
        let topic = Topic::new(RequestTopic::SoftwareUpdateRequest.as_str())?;
        let update_software = SmartRestUpdateSoftware::default();
        let mut software_update_request = update_software
            .from_smartrest(smartrest)?
            .to_thin_edge_json()?;

        let token = self.http_proxy.get_jwt_token().await?;

        software_update_request
            .update_list
            .iter_mut()
            .for_each(|modules| {
                modules.modules.iter_mut().for_each(|module| {
                    if let Some(url) = &module.url {
                        if self.http_proxy.url_is_in_my_tenant_domain(url.url()) {
                            module.url = module.url.as_ref().map(|s| {
                                DownloadInfo::new(&s.url)
                                    .with_auth(Auth::new_bearer(&token.token()))
                            });
                        } else {
                            module.url = module.url.as_ref().map(|s| DownloadInfo::new(&s.url));
                        }
                    }
                });
            });

        let () = self
            .publish(&topic, software_update_request.to_json()?)
            .await?;

        Ok(())
    }

    async fn forward_restart_request(
        &mut self,
        smartrest: &str,
    ) -> Result<(), SMCumulocityMapperError> {
        let topic = Topic::new(RequestTopic::RestartRequest.as_str())?;
        let _ = SmartRestRestartRequest::from_smartrest(smartrest)?;

        let request = RestartOperationRequest::new();
        let () = self.publish(&topic, request.to_json()?).await?;

        Ok(())
    }

    async fn publish(&mut self, topic: &Topic, payload: String) -> Result<(), MqttError> {
        let () = self
            .client
            .published
            .send(mqtt_channel::Message::new(topic, payload))
            .await?;
        Ok(())
    }

    async fn send_software_list_http(
        &mut self,
        json_response: &SoftwareListResponse,
    ) -> Result<(), SMCumulocityMapperError> {
        let c8y_software_list: C8yUpdateSoftwareListResponse = json_response.into();
        self.http_proxy
            .send_software_list_http(&c8y_software_list)
            .await
    }
}

async fn execute_operation(payload: &str, command: &str) -> Result<(), SMCumulocityMapperError> {
    let command = command.to_owned();
    let payload = payload.to_string();

    let _handle = tokio::spawn(async move {
        let mut child = tokio::process::Command::new(command)
            .args(&[payload])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| SMCumulocityMapperError::ExecuteFailed(e.to_string()))
            .unwrap();

        child.wait().await
    });

    Ok(())
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
/// used to retrieve the id of a log event
pub struct SmartRestLogEvent {
    pub id: String,
}

/// Returns a date time object from a file path or file-path-like string
/// a typical file stem looks like this: "software-list-2021-10-27T10:29:58Z"
///
/// # Examples:
/// ```
/// let path_buf = PathBuf::fromStr("/path/to/file/with/date/in/path").unwrap();
/// let path_bufdate_time = get_datetime_from_file_path(&path_buf).unwrap();
/// ```
fn get_datetime_from_file_path(
    log_path: &PathBuf,
) -> Result<DateTime<FixedOffset>, SMCumulocityMapperError> {
    if let Some(stem_string) = log_path.file_stem().and_then(|s| s.to_str()) {
        // a typical file stem looks like this: software-list-2021-10-27T10:29:58Z.
        // to extract the date, rsplit string on "-" and take (last) 3
        let mut stem_string_vec = stem_string.rsplit('-').take(3).collect::<Vec<_>>();
        // reverse back the order (because of rsplit)
        stem_string_vec.reverse();
        // join on '-' to get the date string
        let date_string = stem_string_vec.join("-");
        let dt = DateTime::parse_from_rfc3339(&date_string)?;

        return Ok(dt);
    }
    match log_path.to_str() {
        Some(path) => Err(SMCumulocityMapperError::InvalidDateInFileName(
            path.to_string(),
        )),
        None => Err(SMCumulocityMapperError::InvalidUtf8Path),
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
fn read_tedge_logs(
    smartrest_obj: &SmartRestLogRequest,
    logs_dir: &str,
) -> Result<String, SMCumulocityMapperError> {
    let mut output = String::new();

    // NOTE: As per documentation of std::fs::read_dir:
    // "The order in which this iterator returns entries is platform and filesystem dependent."
    // Therefore, files are sorted by date.
    let mut read_vector: Vec<_> = std::fs::read_dir(logs_dir)?
        .filter_map(|r| r.ok())
        .filter_map(|dir_entry| {
            let file_path = &dir_entry.path();
            let datetime_object = get_datetime_from_file_path(&file_path);
            match datetime_object {
                Ok(dt) => {
                    if dt < smartrest_obj.date_from || dt > smartrest_obj.date_to {
                        return None;
                    }
                    Some(dir_entry)
                }
                Err(_) => None,
            }
        })
        .collect();

    read_vector.sort_by_key(|dir| dir.path());

    // loop sorted vector and push store log file to `output`
    let mut line_counter: usize = 0;
    for entry in read_vector {
        let file_path = entry.path();
        let file_content = std::fs::read_to_string(&file_path)?;
        if file_content.is_empty() {
            continue;
        }

        // adding file header only if line_counter permits more lines to be added
        match &file_path.file_stem().and_then(|f| f.to_str()) {
            Some(file_name) if line_counter < smartrest_obj.lines => {
                output.push_str(&format!("filename: {}\n", file_name));
            }
            _ => {}
        }

        // split at new line delimiter ("\n")
        let mut lines = file_content.lines();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::str::FromStr;
    use test_case::test_case;

    #[test_case("/path/to/software-list-2021-10-27T10:44:44Z.log")]
    #[test_case("/path/to/tedge/agent/software-update-2021-10-25T07:45:41Z.log")]
    #[test_case("/path/to/another-variant-2021-10-25T07:45:41Z.log")]
    #[test_case("/yet-another-variant-2021-10-25T07:45:41Z.log")]
    fn test_datetime_parsing_from_path(file_path: &str) {
        // checking that `get_date_from_file_path` unwraps a `chrono::NaiveDateTime` object.
        // this should return an Ok Result.
        let path_buf = PathBuf::from_str(file_path).unwrap();
        let path_buf_datetime = get_datetime_from_file_path(&path_buf);
        assert!(path_buf_datetime.is_ok());
    }

    #[test_case("/path/to/software-list-2021-10-27-10:44:44Z.log")]
    #[test_case("/path/to/tedge/agent/software-update-10-25-2021T07:45:41Z.log")]
    #[test_case("/path/to/another-variant-07:45:41Z-2021-10-25T.log")]
    #[test_case("/yet-another-variant-2021-10-25T07:45Z.log")]
    fn test_datetime_parsing_from_path_fail(file_path: &str) {
        // checking that `get_date_from_file_path` unwraps a `chrono::NaiveDateTime` object.
        // this should return an err.
        let path_buf = PathBuf::from_str(file_path).unwrap();
        let path_buf_datetime = get_datetime_from_file_path(&path_buf);
        assert!(path_buf_datetime.is_err());
    }

    fn parse_file_names_from_log_content(log_content: &str) -> [&str; 5] {
        let mut files: Vec<&str> = vec![];
        for line in log_content.lines() {
            if line.contains("filename: ") {
                let filename: &str = line.split("filename: ").last().unwrap();
                files.push(filename);
            }
        }
        match files.try_into() {
            Ok(arr) => arr,
            Err(_) => panic!("Could not convert to Array &str, size 5"),
        }
    }

    #[test]
    /// testing read_tedge_logs
    ///
    /// this test creates 5 fake log files in a temporary directory.
    /// files are dated 2021-01-0XT01:00Z, where X = a different day.
    ///
    /// this tests will assert that files are read alphanumerically from oldest to newest
    fn test_read_logs() {
        // order in which files are created
        const LOG_FILE_NAMES: [&str; 5] = [
            "software-list-2021-01-03T01:00:00Z.log",
            "software-list-2021-01-02T01:00:00Z.log",
            "software-list-2021-01-01T01:00:00Z.log",
            "software-update-2021-01-03T01:00:00Z.log",
            "software-update-2021-01-02T01:00:00Z.log",
        ];

        // expected (sorted) output
        const EXPECTED_OUTPUT: [&str; 5] = [
            "software-list-2021-01-01T01:00:00Z",
            "software-list-2021-01-02T01:00:00Z",
            "software-list-2021-01-03T01:00:00Z",
            "software-update-2021-01-02T01:00:00Z",
            "software-update-2021-01-03T01:00:00Z",
        ];

        let smartrest_obj = SmartRestLogRequest::from_smartrest(
            "522,DeviceSerial,syslog,2021-01-01T00:00:00+0200,2021-01-10T00:00:00+0200,,1000",
        )
        .unwrap();

        let temp_dir = tempfile::tempdir().unwrap();
        // creating the files
        for (idx, file) in LOG_FILE_NAMES.iter().enumerate() {
            let file_path = &temp_dir.path().join(file);
            let mut file = File::create(file_path).unwrap();
            writeln!(file, "file num {}", idx).unwrap();
        }

        // reading the logs and extracting the file names from the log output.
        let output = read_tedge_logs(&smartrest_obj, temp_dir.path().to_str().unwrap()).unwrap();
        let parsed_values = parse_file_names_from_log_content(&output);

        // asserting the order = `EXPECTED_OUTPUT`
        assert!(parsed_values.eq(&EXPECTED_OUTPUT));
    }
}
