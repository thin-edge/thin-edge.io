use crate::sm_c8y_mapper::error::SMCumulocityMapperError;
use crate::sm_c8y_mapper::json_c8y::{
    C8yCreateEvent, C8yManagedObject, C8yUpdateSoftwareListResponse, InternalIdResponse,
};
use crate::sm_c8y_mapper::mapper::SmartRestLogEvent;
use async_trait::async_trait;
use c8y_smartrest::smartrest_deserializer::SmartRestJwtResponse;
use chrono::{DateTime, Local};
use mqtt_client::{Client, MqttClient, Topic};
use reqwest::Url;
use std::time::Duration;
use tedge_config::{C8yUrlSetting, ConfigSettingAccessorStringExt, DeviceIdSetting, TEdgeConfig};
use tracing::{error, info, instrument};

const RETRY_TIMEOUT_SECS: u64 = 60;

/// An HttpProxy handles http requests to C8y on behalf of the device.
#[async_trait]
pub trait C8YHttpProxy {
    async fn init(&mut self) -> Result<(), SMCumulocityMapperError>;

    fn url_is_in_my_tenant_domain(&self, url: &str) -> bool;

    async fn get_jwt_token(&self) -> Result<SmartRestJwtResponse, SMCumulocityMapperError>;

    async fn send_software_list_http(
        &self,
        c8y_software_list: &C8yUpdateSoftwareListResponse,
    ) -> Result<(), SMCumulocityMapperError>;

    async fn upload_log_binary(&self, log_content: &str)
        -> Result<String, SMCumulocityMapperError>;

    fn create_log_event(&self) -> C8yCreateEvent;

    async fn get_event_id(&self, event: C8yCreateEvent) -> Result<String, SMCumulocityMapperError>;
}

/// Define a C8y endpoint
pub struct C8yEndPoint {
    c8y_host: String,
    device_id: String,
    c8y_internal_id: String,
}

impl C8yEndPoint {
    fn get_url_for_sw_list(&self) -> String {
        let mut url_update_swlist = String::new();
        url_update_swlist.push_str("https://");
        url_update_swlist.push_str(&self.c8y_host);
        url_update_swlist.push_str("/inventory/managedObjects/");
        url_update_swlist.push_str(&self.c8y_internal_id);

        url_update_swlist
    }

    fn get_url_for_get_id(&self) -> String {
        let mut url_get_id = String::new();
        url_get_id.push_str("https://");
        url_get_id.push_str(&self.c8y_host);
        url_get_id.push_str("/identity/externalIds/c8y_Serial/");
        url_get_id.push_str(&self.device_id);

        url_get_id
    }

    fn get_url_for_create_event(&self) -> String {
        let mut url_create_event = String::new();
        url_create_event.push_str("https://");
        url_create_event.push_str(&self.c8y_host);
        url_create_event.push_str("/event/events/");

        url_create_event
    }

    fn get_url_for_event_binary_upload(&self, event_id: &str) -> String {
        let mut url_event_binary = self.get_url_for_create_event();
        url_event_binary.push_str(event_id);
        url_event_binary.push_str("/binaries");

        url_event_binary
    }

    fn url_is_in_my_tenant_domain(&self, url: &str) -> bool {
        // c8y URL may contain either `Tenant Name` or Tenant Id` so they can be one of following options:
        // * <tenant_name>.<domain> eg: sample.c8y.io
        // * <tenant_id>.<domain> eg: t12345.c8y.io
        // These URLs may be both equivalent and point to the same tenant.
        // We are going to remove that and only check if the domain is the same.
        let tenant_uri = &self.c8y_host;
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
}

/// An HttpProxy that uses MQTT to retrieve JWT tokens and authenticate the device
///
/// - Keep the connection info to c8y and the internal Id of the device
/// - Handle JWT requests
pub struct MqttAuthHttpProxy {
    mqtt_con: Client,
    end_point: C8yEndPoint,
}

impl MqttAuthHttpProxy {
    pub fn try_new(
        mqtt_con: Client,
        tedge_config: &TEdgeConfig,
    ) -> Result<MqttAuthHttpProxy, SMCumulocityMapperError> {
        let c8y_host = tedge_config.query_string(C8yUrlSetting)?;
        let device_id = tedge_config.query_string(DeviceIdSetting)?;
        Ok(MqttAuthHttpProxy {
            mqtt_con,
            end_point: C8yEndPoint {
                c8y_host,
                device_id,
                c8y_internal_id: "".into(),
            },
        })
    }

    async fn try_get_and_set_internal_id(&mut self) -> Result<(), SMCumulocityMapperError> {
        let token = self.get_jwt_token().await?;
        let reqwest_client = reqwest::ClientBuilder::new().build()?;
        let url_get_id = self.end_point.get_url_for_get_id();

        self.end_point.c8y_internal_id =
            MqttAuthHttpProxy::try_get_internal_id(&reqwest_client, &url_get_id, &token.token())
                .await?;

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
}

#[async_trait]
impl C8YHttpProxy for MqttAuthHttpProxy {
    fn url_is_in_my_tenant_domain(&self, url: &str) -> bool {
        self.end_point.url_is_in_my_tenant_domain(url)
    }

    #[instrument(skip(self), name = "init")]
    async fn init(&mut self) -> Result<(), SMCumulocityMapperError> {
        info!("Initialisation");
        while self.end_point.c8y_internal_id.is_empty() {
            if let Err(error) = self.try_get_and_set_internal_id().await {
                error!(
                    "An error ocurred while retrieving internal Id, operation will retry in {} seconds and mapper will reinitialise.\n Error: {:?}",
                    RETRY_TIMEOUT_SECS, error
                );

                tokio::time::sleep(Duration::from_secs(RETRY_TIMEOUT_SECS)).await;
                continue;
            };
        }
        info!("Initialisation done.");
        Ok(())
    }

    async fn get_jwt_token(&self) -> Result<SmartRestJwtResponse, SMCumulocityMapperError> {
        let mut subscriber = self
            .mqtt_con
            .subscribe(Topic::new("c8y/s/dat")?.filter())
            .await?;

        let () = self
            .mqtt_con
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

    async fn send_software_list_http(
        &self,
        c8y_software_list: &C8yUpdateSoftwareListResponse,
    ) -> Result<(), SMCumulocityMapperError> {
        let url = self.end_point.get_url_for_sw_list();
        let client = reqwest::ClientBuilder::new().build()?;
        let token = self.get_jwt_token().await?;

        let request = client
            .put(url)
            .json(c8y_software_list)
            .bearer_auth(&token.token())
            .timeout(Duration::from_millis(10000))
            .build()?;

        let _response = client.execute(request).await?;

        Ok(())
    }

    /// Make a POST request to /event/events and return the event id from response body.
    /// The event id is used to upload the binary.
    fn create_log_event(&self) -> C8yCreateEvent {
        let local: DateTime<Local> = Local::now();

        let c8y_managed_object = C8yManagedObject {
            id: self.end_point.c8y_internal_id.clone(),
        };

        C8yCreateEvent::new(
            c8y_managed_object.to_owned(),
            "c8y_Logfile",
            &local.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "software-management",
        )
    }

    async fn get_event_id(
        &self,
        c8y_event: C8yCreateEvent,
    ) -> Result<String, SMCumulocityMapperError> {
        let client = reqwest::ClientBuilder::new().build()?;
        let token = self.get_jwt_token().await?;
        let create_event_url = self.end_point.get_url_for_create_event();

        let request = client
            .post(create_event_url)
            .json(&c8y_event)
            .bearer_auth(token.token())
            .header("Accept", "application/json")
            .timeout(Duration::from_millis(10000))
            .build()?;

        let response = client.execute(request).await?;
        let event_response_body = response.json::<SmartRestLogEvent>().await?;

        Ok(event_response_body.id)
    }

    async fn upload_log_binary(
        &self,
        log_content: &str,
    ) -> Result<String, SMCumulocityMapperError> {
        let client = reqwest::ClientBuilder::new().build()?;
        let token = self.get_jwt_token().await?;

        let log_event = self.create_log_event();
        let event_response_id = self.get_event_id(log_event).await?;
        let binary_upload_event_url = self
            .end_point
            .get_url_for_event_binary_upload(&event_response_id);

        let request = client
            .post(&binary_upload_event_url)
            .header("Accept", "application/json")
            .header("Content-Type", "text/plain")
            .body(log_content.to_string())
            .bearer_auth(token.token())
            .timeout(Duration::from_millis(10000))
            .build()?;

        let _response = client.execute(request).await?;
        Ok(binary_upload_event_url)
    }
}
