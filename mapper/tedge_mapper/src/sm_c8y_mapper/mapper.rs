use crate::mapper::mqtt_config;
use crate::sm_c8y_mapper::json_c8y::{C8yCreateEvent, C8yManagedObject};
use crate::sm_c8y_mapper::{error::*, json_c8y::C8yUpdateSoftwareListResponse, topic::*};
use crate::{component::TEdgeComponent, sm_c8y_mapper::json_c8y::InternalIdResponse};
use async_trait::async_trait;
use c8y_smartrest::smartrest_deserializer::SmartRestLogRequest;
use c8y_smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_smartrest::{
    error::SmartRestDeserializerError,
    smartrest_deserializer::{SmartRestJwtResponse, SmartRestUpdateSoftware},
    smartrest_serializer::{
        SmartRestGetPendingOperations, SmartRestSerializer, SmartRestSetOperationToExecuting,
        SmartRestSetOperationToFailed, SmartRestSetOperationToSuccessful,
        SmartRestSetSupportedLogType, SmartRestSetSupportedOperations,
    },
};
use chrono::{DateTime, FixedOffset, Local};
use json_sm::{
    Auth, DownloadInfo, Jsonify, SoftwareListRequest, SoftwareListResponse,
    SoftwareOperationStatus, SoftwareUpdateResponse,
};
use mqtt_client::{Client, MqttClient, MqttClientError, MqttMessageStream, Topic, TopicFilter};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::{convert::TryInto, time::Duration};
use tedge_config::{C8yUrlSetting, ConfigSettingAccessorStringExt, DeviceIdSetting, TEdgeConfig};
use tokio::time::Instant;
use tracing::{debug, error, info, instrument};

const AGENT_LOG_DIR: &str = "/var/log/tedge/agent";

const RETRY_TIMEOUT_SECS: u64 = 60;


pub struct CumulocitySoftwareManagementMapper {}

impl CumulocitySoftwareManagementMapper {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl TEdgeComponent for CumulocitySoftwareManagementMapper {
    #[instrument(skip(self, tedge_config), name = "sm-c8y-mapper")]
    async fn start(&self, tedge_config: TEdgeConfig) -> Result<(), anyhow::Error> {
        let mqtt_config = mqtt_config(&tedge_config)?;
        let mqtt_client = Client::connect("SM-C8Y-Mapper", &mqtt_config).await?;

        let mut sm_mapper = CumulocitySoftwareManagement::new(mqtt_client, tedge_config);
        let mut topic_filter = TopicFilter::new(IncomingTopic::SoftwareListResponse.as_str())?;
        topic_filter.add(IncomingTopic::SoftwareUpdateResponse.as_str())?;
        topic_filter.add(IncomingTopic::SmartRestRequest.as_str())?;
        let messages = sm_mapper.client.subscribe(topic_filter).await?;

        let () = sm_mapper.init().await?;
        let () = sm_mapper.run(messages).await?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct CumulocitySoftwareManagement {
    pub client: Client,
    config: TEdgeConfig,
    c8y_internal_id: String,
}

impl CumulocitySoftwareManagement {
    pub fn new(client: Client, config: TEdgeConfig) -> Self {
        Self {
            client,
            config,
            c8y_internal_id: "".into(),
        }
    }

    #[instrument(skip(self), name = "init")]
    async fn init(&mut self) -> Result<(), anyhow::Error> {
        info!("Initialisation");
        while self.c8y_internal_id.is_empty() {
            if let Err(error) = self.try_get_and_set_internal_id().await {
                error!(
                    "An error ocurred while retrieving internal Id, operation will retry in {} seconds and mapper will reinitialise.\n Error: {:?}",
                    RETRY_TIMEOUT_SECS, error
                );

                tokio::time::sleep_until(Instant::now() + Duration::from_secs(RETRY_TIMEOUT_SECS))
                    .await;
                continue;
            };
        }
        info!("Initialisation done.");

        Ok(())
    }

    pub async fn run(&self, mut messages: Box<dyn MqttMessageStream>) -> Result<(), anyhow::Error> {
        info!("Running");
        let () = self.publish_supported_operations().await?;
        let () = self.publish_supported_log_types().await?;
        let () = self.publish_get_pending_operations().await?;
        let () = self.ask_software_list().await?;

        while let Err(err) = self.subscribe_messages_runtime(&mut messages).await {
            if let SMCumulocityMapperError::FromSmartRestDeserializer(
                SmartRestDeserializerError::InvalidParameter { operation, .. },
            ) = &err
            {
                let topic = OutgoingTopic::SmartRestResponse.to_topic()?;
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

    async fn process_smartrest(&self, payload: &str) -> Result<(), SMCumulocityMapperError> {
        let message_id: &str = &payload[..3];
        match message_id {
            "528" => {
                let () = self.forward_software_request(payload).await?;
            }
            "522" => {
                let () = self.forward_log_request(payload).await?;
            }
            _ => {
                return Err(SMCumulocityMapperError::InvalidMqttMessage);
            }
        }

        Ok(())
    }

    #[instrument(skip(self, messages), name = "main-loop")]
    async fn subscribe_messages_runtime(
        &self,
        messages: &mut Box<dyn MqttMessageStream>,
    ) -> Result<(), SMCumulocityMapperError> {
        while let Some(message) = messages.next().await {
            debug!("Topic {:?}", message.topic.name);
            debug!("Mapping {:?}", message.payload_str());

            let incoming_topic = message.topic.clone().try_into()?;
            match incoming_topic {
                IncomingTopic::SoftwareListResponse => {
                    debug!("Software list");
                    let () = self
                        .validate_and_publish_software_list(message.payload_str()?)
                        .await?;
                }
                IncomingTopic::SoftwareUpdateResponse => {
                    debug!("Software update");
                    let () = self
                        .publish_operation_status(message.payload_str()?)
                        .await?;
                }
                IncomingTopic::SmartRestRequest => {
                    debug!("Cumulocity");
                    let () = self.process_smartrest(message.payload_str()?).await?;
                }
            }
        }
        Ok(())
    }

    #[instrument(skip(self), name = "software-list")]
    async fn ask_software_list(&self) -> Result<(), SMCumulocityMapperError> {
        let request = SoftwareListRequest::new();
        let topic = OutgoingTopic::SoftwareListRequest.to_topic()?;
        let json_list_request = request.to_json()?;
        let () = self.publish(&topic, json_list_request).await?;

        Ok(())
    }

    #[instrument(skip(self), name = "software-update")]
    async fn validate_and_publish_software_list(
        &self,
        json_response: &str,
    ) -> Result<(), SMCumulocityMapperError> {
        let response = SoftwareListResponse::from_json(json_response)?;

        match response.status() {
            SoftwareOperationStatus::Successful => {
                let () = self.send_software_list_http(&response).await?;
            }

            SoftwareOperationStatus::Failed => {
                error!("Received a failed software response: {}", json_response);
            }

            SoftwareOperationStatus::Executing => {} // C8Y doesn't expect any message to be published
        }

        Ok(())
    }

    async fn publish_supported_log_types(&self) -> Result<(), SMCumulocityMapperError> {
        let payload = SmartRestSetSupportedLogType::default().to_smartrest()?;
        let topic = OutgoingTopic::SmartRestResponse.to_topic()?;
        let () = self.publish(&topic, payload).await?;
        Ok(())
    }

    async fn publish_supported_operations(&self) -> Result<(), SMCumulocityMapperError> {
        let data = SmartRestSetSupportedOperations::default();
        let topic = OutgoingTopic::SmartRestResponse.to_topic()?;
        let payload = data.to_smartrest()?;
        let () = self.publish(&topic, payload).await?;
        Ok(())
    }

    async fn publish_get_pending_operations(&self) -> Result<(), SMCumulocityMapperError> {
        let data = SmartRestGetPendingOperations::default();
        let topic = OutgoingTopic::SmartRestResponse.to_topic()?;
        let payload = data.to_smartrest()?;
        let () = self.publish(&topic, payload).await?;
        Ok(())
    }

    async fn publish_operation_status(
        &self,
        json_response: &str,
    ) -> Result<(), SMCumulocityMapperError> {
        let response = SoftwareUpdateResponse::from_json(json_response)?;
        let topic = OutgoingTopic::SmartRestResponse.to_topic()?;
        match response.status() {
            SoftwareOperationStatus::Executing => {
                let smartrest_set_operation_status =
                    SmartRestSetOperationToExecuting::from_thin_edge_json(response)?
                        .to_smartrest()?;
                let () = self.publish(&topic, smartrest_set_operation_status).await?;
            }
            SoftwareOperationStatus::Successful => {
                let smartrest_set_operation =
                    SmartRestSetOperationToSuccessful::from_thin_edge_json(response)?
                        .to_smartrest()?;
                let () = self.publish(&topic, smartrest_set_operation).await?;
                let () = self
                    .validate_and_publish_software_list(json_response)
                    .await?;
            }
            SoftwareOperationStatus::Failed => {
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

    async fn set_log_file_request_executing(&self) -> Result<(), SMCumulocityMapperError> {
        let topic = OutgoingTopic::SmartRestResponse.to_topic()?;
        let smartrest_set_operation_status =
            SmartRestSetOperationToExecuting::new(CumulocitySupportedOperations::C8yLogFileRequest)
                .to_smartrest()?;

        let () = self.publish(&topic, smartrest_set_operation_status).await?;
        Ok(())
    }

    async fn set_log_file_request_done(
        &self,
        binary_upload_event_url: &str,
    ) -> Result<(), SMCumulocityMapperError> {
        let topic = OutgoingTopic::SmartRestResponse.to_topic()?;
        let smartrest_set_operation_status = SmartRestSetOperationToSuccessful::new(
            CumulocitySupportedOperations::C8yLogFileRequest,
        )
        .with_response_parameter(binary_upload_event_url)
        .to_smartrest()?;

        let () = self.publish(&topic, smartrest_set_operation_status).await?;
        Ok(())
    }

    async fn forward_log_request(&self, payload: &str) -> Result<(), SMCumulocityMapperError> {
        // retrieve smartrest object from payload
        let smartrest_obj = SmartRestLogRequest::from_smartrest(&payload)?;

        // 1. set log file request to executing
        let () = self.set_log_file_request_executing().await?;

        // 2. read logs
        let log_output = read_tedge_logs(&smartrest_obj, AGENT_LOG_DIR)?;

        // 3. create log event
        let token = get_jwt_token(&self.client).await?;
        let url_host = self.config.query_string(C8yUrlSetting)?;

        let c8y_managed_object = C8yManagedObject {
            id: self.c8y_internal_id.clone(),
        };
        let event_response_id = create_log_event(&url_host, &c8y_managed_object, &token).await?;

        // 4. upload log file
        let binary_upload_event_url =
            get_url_for_event_binary_upload(&url_host, &event_response_id);

        let () = upload_log_binary(&token, &binary_upload_event_url, &log_output.as_str()).await?;

        // 5. set log file request to done
        let () = self
            .set_log_file_request_done(&binary_upload_event_url)
            .await?;

        info!("Log file request uploaded");

        Ok(())
    }

    async fn forward_software_request(
        &self,
        smartrest: &str,
    ) -> Result<(), SMCumulocityMapperError> {
        let topic = OutgoingTopic::SoftwareUpdateRequest.to_topic()?;
        let update_software = SmartRestUpdateSoftware::new();
        let mut software_update_request = update_software
            .from_smartrest(smartrest)?
            .to_thin_edge_json()?;

        let token = get_jwt_token(&self.client).await?;
        let tenant_uri = self.config.query_string(C8yUrlSetting)?;

        software_update_request
            .update_list
            .iter_mut()
            .for_each(|modules| {
                modules.modules.iter_mut().for_each(|module| {
                    if let Some(url) = &module.url {
                        if url_is_in_my_tenant_domain(url.url(), &tenant_uri) {
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

    async fn publish(&self, topic: &Topic, payload: String) -> Result<(), MqttClientError> {
        let () = self
            .client
            .publish(mqtt_client::Message::new(topic, payload))
            .await?;
        Ok(())
    }

    async fn send_software_list_http(
        &self,
        json_response: &SoftwareListResponse,
    ) -> Result<(), SMCumulocityMapperError> {
        let token = get_jwt_token(&self.client).await?;

        let reqwest_client = reqwest::ClientBuilder::new().build()?;

        let url_host = self.config.query_string(C8yUrlSetting)?;
        let url = get_url_for_sw_list(&url_host, &self.c8y_internal_id);

        let c8y_software_list: C8yUpdateSoftwareListResponse = json_response.into();

        let _published =
            publish_software_list_http(&reqwest_client, &url, &token.token(), &c8y_software_list)
                .await?;

        Ok(())
    }

    async fn try_get_and_set_internal_id(&mut self) -> Result<(), SMCumulocityMapperError> {
        let token = get_jwt_token(&self.client).await?;
        let reqwest_client = reqwest::ClientBuilder::new().build()?;

        let url_host = self.config.query_string(C8yUrlSetting)?;
        let device_id = self.config.query_string(DeviceIdSetting)?;
        let url_get_id = get_url_for_get_id(&url_host, &device_id);

        self.c8y_internal_id =
            try_get_internal_id(&reqwest_client, &url_get_id, &token.token()).await?;

        Ok(())
    }
}

async fn upload_log_binary(
    token: &SmartRestJwtResponse,
    binary_upload_event_url: &str,
    log_content: &str,
) -> Result<(), SMCumulocityMapperError> {
    let client = reqwest::ClientBuilder::new().build()?;

    let request = client
        .post(binary_upload_event_url)
        .header("Accept", "application/json")
        .header("Content-Type", "text/plain")
        .body(log_content.to_string())
        .bearer_auth(token.token())
        .timeout(Duration::from_millis(10000))
        .build()?;

    let _response = client.execute(request).await?;
    Ok(())
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
/// used to retrieve the id of a log event
pub struct SmartRestLogEvent {
    pub id: String,
}

/// Make a POST request to /event/events and return the event id from response body.
/// The event id is used to upload the binary.
async fn create_log_event(
    url_host: &str,
    c8y_managed_object: &C8yManagedObject,
    token: &SmartRestJwtResponse,
) -> Result<String, SMCumulocityMapperError> {
    let client = reqwest::ClientBuilder::new().build()?;

    let create_event_url = get_url_for_create_event(&url_host);

    let local: DateTime<Local> = Local::now();

    let c8y_log_event = C8yCreateEvent::new(
        c8y_managed_object.to_owned(),
        "c8y_Logfile",
        &local.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "software-management",
    );

    let request = client
        .post(create_event_url)
        .json(&c8y_log_event)
        .bearer_auth(token.token())
        .header("Accept", "application/json")
        .timeout(Duration::from_millis(10000))
        .build()?;

    let response = client.execute(request).await?;
    let event_response_body = response.json::<SmartRestLogEvent>().await?;

    Ok(event_response_body.id)
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

    Err(SMCumulocityMapperError::InvalidDateInFileName(
        log_path.to_str().unwrap().to_string(),
    ))
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

fn url_is_in_my_tenant_domain(url: &str, tenant_uri: &str) -> bool {
    // c8y URL may contain either `Tenant Name` or Tenant Id` so they can be one of following options:
    // * <tenant_name>.<domain> eg: sample.c8y.io
    // * <tenant_id>.<domain> eg: t12345.c8y.io
    // These URLs may be both equivalent and point to the same tenant.
    // We are going to remove that and only check if the domain is the same.
    let url_host = match Url::parse(url) {
        Ok(url) => match url.host() {
            Some(host) => host.to_string(),
            None => return false,
        },
        Err(_err) => {
            return false;
        }
    };

    let url_domain = url_host.splitn(2, '.').collect::<Vec<&str>>();
    let tenant_domain = tenant_uri.splitn(2, '.').collect::<Vec<&str>>();
    if url_domain.get(1) == tenant_domain.get(1) {
        return true;
    }
    false
}

async fn publish_software_list_http(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    list: &C8yUpdateSoftwareListResponse,
) -> Result<(), SMCumulocityMapperError> {
    let request = client
        .put(url)
        .json(list)
        .bearer_auth(token)
        .timeout(Duration::from_millis(10000))
        .build()?;

    let _response = client.execute(request).await?;

    Ok(())
}

async fn try_get_internal_id(
    client: &reqwest::Client,
    url_get_id: &str,
    token: &str,
) -> Result<String, SMCumulocityMapperError> {
    let internal_id = client.get(url_get_id).bearer_auth(token).send().await?;
    let internal_id_response = internal_id.json::<InternalIdResponse>().await?;

    let internal_id = internal_id_response.id();
    Ok(internal_id)
}

fn get_url_for_sw_list(url_host: &str, internal_id: &str) -> String {
    let mut url_update_swlist = String::new();
    url_update_swlist.push_str("https://");
    url_update_swlist.push_str(url_host);
    url_update_swlist.push_str("/inventory/managedObjects/");
    url_update_swlist.push_str(internal_id);

    url_update_swlist
}

fn get_url_for_get_id(url_host: &str, device_id: &str) -> String {
    let mut url_get_id = String::new();
    url_get_id.push_str("https://");
    url_get_id.push_str(url_host);
    url_get_id.push_str("/identity/externalIds/c8y_Serial/");
    url_get_id.push_str(device_id);

    url_get_id
}

fn get_url_for_create_event(url_host: &str) -> String {
    let mut url_create_event = String::new();
    url_create_event.push_str("https://");
    url_create_event.push_str(url_host);
    url_create_event.push_str("/event/events/");

    url_create_event
}

fn get_url_for_event_binary_upload(url_host: &str, event_id: &str) -> String {
    let mut url_event_binary = get_url_for_create_event(url_host);
    url_event_binary.push_str(event_id);
    url_event_binary.push_str("/binaries");

    url_event_binary
}

async fn get_jwt_token(client: &Client) -> Result<SmartRestJwtResponse, SMCumulocityMapperError> {
    let mut subscriber = client.subscribe(Topic::new("c8y/s/dat")?.filter()).await?;

    let () = client
        .publish(mqtt_client::Message::new(
            &Topic::new("c8y/s/uat")?,
            "".to_string(),
        ))
        .await?;

    let token_smartrest =
        match tokio::time::timeout(Duration::from_secs(10), subscriber.next()).await {
            Ok(Some(msg)) => msg.payload_str()?.to_string(),
            Ok(None) => return Err(SMCumulocityMapperError::InvalidMqttMessage),
            Err(_elapsed) => return Err(SMCumulocityMapperError::RequestTimeout),
        };

    Ok(SmartRestJwtResponse::try_new(&token_smartrest)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_client::MqttMessageStream;
    use std::fs::File;
    use std::io::Write;
    use std::{str::FromStr, sync::Arc};
    use test_case::test_case;

    const MQTT_TEST_PORT: u16 = 55555;
    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(2000);

    #[tokio::test]
    #[cfg_attr(not(feature = "mosquitto-available"), ignore)]
    async fn get_jwt_token_full_run() {
        // Prepare subscribers to listen on messages on topic `c8y/s/us` where we expect to receive empty message.
        let mut publish_messages_stream =
            get_subscriber("c8y/s/uat", "get_jwt_token_full_run_sub1").await;

        let publisher = Arc::new(
            Client::connect(
                "get_jwt_token_full_run",
                &mqtt_client::Config::default().with_port(MQTT_TEST_PORT),
            )
            .await
            .unwrap(),
        );

        let publisher2 = publisher.clone();

        // Setup listener stream to publish on first message received on topic `c8y/s/us`.
        let responder_task = tokio::spawn(async move {
            match tokio::time::timeout(TEST_TIMEOUT_MS, publish_messages_stream.next()).await {
                Ok(Some(msg)) => {
                    // When first messages is received assert it is on `c8y/s/us` topic and it has empty payload.
                    assert_eq!(msg.topic, Topic::new("c8y/s/uat").unwrap());
                    assert_eq!(msg.payload_str().unwrap(), "");

                    // After receiving successful message publish response with a custom 'token' on topic `c8y/s/dat`.
                    let message =
                        mqtt_client::Message::new(&Topic::new("c8y/s/dat").unwrap(), "71,1111");
                    let _ = publisher2.publish(message).await;
                }
                _ => panic!("No message received after a second."),
            }
        });

        // Wait till token received.
        let (jwt_token, _responder) = tokio::join!(get_jwt_token(&publisher), responder_task);

        // `get_jwt_token` should return `Ok` and the value of token should be as set above `1111`.
        assert!(jwt_token.is_ok());
        assert_eq!(jwt_token.unwrap().token(), "1111");
    }

    #[test]
    fn get_url_for_get_id_returns_correct_address() {
        let res = get_url_for_get_id("test_host", "test_device");

        assert_eq!(
            res,
            "https://test_host/identity/externalIds/c8y_Serial/test_device"
        );
    }

    #[test]
    fn get_url_for_sw_list_returns_correct_address() {
        let res = get_url_for_sw_list("test_host", "12345");

        assert_eq!(res, "https://test_host/inventory/managedObjects/12345");
    }

    #[test_case("http://aaa.test.com")]
    #[test_case("https://aaa.test.com")]
    #[test_case("ftp://aaa.test.com")]
    #[test_case("mqtt://aaa.test.com")]
    #[test_case("https://t1124124.test.com")]
    #[test_case("https://t1124124.test.com:12345")]
    #[test_case("https://t1124124.test.com/path")]
    #[test_case("https://t1124124.test.com/path/to/file.test")]
    #[test_case("https://t1124124.test.com/path/to/file")]
    fn url_is_my_tenant_correct_urls(url: &str) {
        assert!(url_is_in_my_tenant_domain(url, "test.test.com"));
    }

    #[test_case("test.com")]
    #[test_case("http://test.co")]
    #[test_case("http://test.co.te")]
    #[test_case("http://test.com:123456")]
    #[test_case("http://test.com::12345")]
    fn url_is_my_tenant_incorrect_urls(url: &str) {
        assert!(!url_is_in_my_tenant_domain(url, "test.test.com"));
    }

    async fn get_subscriber(pattern: &str, client_name: &str) -> Box<dyn MqttMessageStream> {
        let topic_filter = TopicFilter::new(pattern).unwrap();
        let subscriber = Client::connect(
            client_name,
            &mqtt_client::Config::default().with_port(MQTT_TEST_PORT),
        )
        .await
        .unwrap();

        // Obtain subscribe stream
        subscriber.subscribe(topic_filter).await.unwrap()
    }

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
