use crate::mapper::mqtt_config;
use crate::sm_c8y_mapper::{
    error::*, json_c8y::C8yUpdateSoftwareListResponse, smartrest_deserializer::*,
    smartrest_serializer::*, topic::*,
};
use crate::{component::TEdgeComponent, sm_c8y_mapper::json_c8y::InternalIdResponse};
use async_trait::async_trait;
use json_sm::{
    Jsonify, SoftwareListRequest, SoftwareListResponse, SoftwareOperationStatus,
    SoftwareUpdateResponse,
};
use mqtt_client::{Client, MqttClient, MqttClientError, Topic, TopicFilter};
use std::{convert::TryInto, time::Duration};
use tedge_config::{C8yUrlSetting, ConfigSettingAccessorStringExt, DeviceIdSetting, TEdgeConfig};
use tracing::{debug, error};

pub struct CumulocitySoftwareManagementMapper {}

impl CumulocitySoftwareManagementMapper {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl TEdgeComponent for CumulocitySoftwareManagementMapper {
    async fn start(&self, tedge_config: TEdgeConfig) -> Result<(), anyhow::Error> {
        let mqtt_config = mqtt_config(&tedge_config)?;
        let mqtt_client = Client::connect("SM-C8Y-Mapper", &mqtt_config).await?;

        let mut sm_mapper = CumulocitySoftwareManagement::new(mqtt_client, tedge_config);
        let () = sm_mapper.run().await?;

        Ok(())
    }
}

#[derive(Debug)]
struct CumulocitySoftwareManagement {
    client: Client,
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

    async fn run(&mut self) -> Result<(), anyhow::Error> {
        let () = self.publish_supported_operations().await?;
        let () = self.publish_get_pending_operations().await?;
        let () = self.ask_software_list().await?;

        let token = get_jwt_token(&self.client).await.unwrap();

        let reqwest_client = reqwest::ClientBuilder::new().build().unwrap();

        let url_host = get_url_host_config(&self.config).unwrap();
        let device_id = get_device_id_config(&self.config).unwrap();
        let url_get_id = get_url_for_get_id(&url_host, &device_id).unwrap();

        if self.c8y_internal_id.is_empty() {
            self.c8y_internal_id =
                try_get_internal_id(&reqwest_client, &url_get_id, &token.token())
                    .await
                    .unwrap();
        }

        while let Err(err) = self.subscribe_messages_runtime().await {
            error!("{}", err);
        }

        Ok(())
    }

    async fn subscribe_messages_runtime(&self) -> Result<(), SMCumulocityMapperError> {
        let mut topic_filter = TopicFilter::new(IncomingTopic::SoftwareListResponse.as_str())?;
        topic_filter.add(IncomingTopic::SoftwareUpdateResponse.as_str())?;
        topic_filter.add(IncomingTopic::SmartRestRequest.as_str())?;

        let mut messages = self.client.subscribe(topic_filter).await?;

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
                    let () = self
                        .forward_software_request(message.payload_str()?)
                        .await?;
                }
            }
        }
        Ok(())
    }

    async fn ask_software_list(&self) -> Result<(), SMCumulocityMapperError> {
        let request = SoftwareListRequest::new();
        let topic = OutgoingTopic::SoftwareListRequest.to_topic()?;
        let json_list_request = request.to_json()?;
        let () = self.publish(&topic, json_list_request).await?;

        Ok(())
    }

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

    async fn forward_software_request(
        &self,
        smartrest: &str,
    ) -> Result<(), SMCumulocityMapperError> {
        let topic = OutgoingTopic::SoftwareUpdateRequest.to_topic()?;
        let update_software = SmartRestUpdateSoftware::new();
        let json_update_request = update_software
            .from_smartrest(smartrest.into())?
            .to_thin_edge_json()?
            .to_json()?;
        let () = self.publish(&topic, json_update_request).await?;

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
        let token = get_jwt_token(&self.client).await.unwrap();

        let client = reqwest::ClientBuilder::new().build().unwrap();

        let url_host = get_url_host_config(&self.config).unwrap();

        let c8y_software_list: C8yUpdateSoftwareListResponse = json_response.into();

        let _published = publish_software_list_http(
            &client,
            &url_host,
            &self.c8y_internal_id,
            &token.token(),
            &c8y_software_list,
        )
        .await
        .unwrap();

        // Do we want to retry?
        Ok(())
    }
}

async fn publish_software_list_http(
    client: &reqwest::Client,
    url_host: &str,
    internal_id: &str,
    token: &str,
    list: &C8yUpdateSoftwareListResponse,
) -> Result<(), SMCumulocityMapperError> {
    let url_update_swlist = format!(
        "https://{}/inventory/managedObjects/{}",
        url_host, internal_id
    );

    let payload = list.to_json().unwrap();

    let request = client
        .put(url_update_swlist)
        .header("Authorization", format!("Bearer {}", token))
        .body(payload)
        // .json(payload)
        // .bearer_auth(token.token)
        .timeout(Duration::from_millis(2000))
        .build()
        .unwrap();

    let _response = client.execute(request).await.unwrap();

    Ok(())
}

async fn try_get_internal_id(
    client: &reqwest::Client,
    url_get_id: &str,
    token: &str,
) -> Result<String, SMCumulocityMapperError> {
    let internal_id = client
        .get(url_get_id)
        .bearer_auth(token)
        .send()
        .await
        .unwrap();

    let internal_id_response = internal_id.json::<InternalIdResponse>().await.unwrap();

    let internal_id = internal_id_response.id();
    Ok(internal_id)
}

fn get_url_host_config(config: &TEdgeConfig) -> Result<String, SMCumulocityMapperError> {
    let url_host = config
        .query_string(C8yUrlSetting)
        // TODO: Handle result
        .unwrap();

    Ok(url_host)
}

fn get_device_id_config(config: &TEdgeConfig) -> Result<String, SMCumulocityMapperError> {
    let device_id = config
        .query_string(DeviceIdSetting)
        // TODO: Handle result
        .unwrap();

    Ok(device_id)
}

fn get_url_for_get_id(url_host: &str, device_id: &str) -> Result<String, SMCumulocityMapperError> {
    let url_get_id = format!(
        "https://{}/identity/externalIds/c8y_Serial/{}",
        url_host, device_id
    );

    Ok(url_get_id)
}

async fn get_jwt_token(client: &Client) -> Result<SmartRestJwtResponse, SMCumulocityMapperError> {
    let mut subscriber = client
        .subscribe(Topic::new("c8y/s/dat").unwrap().filter())
        .await
        .unwrap();

    let () = client
        .publish(mqtt_client::Message::new(
            &"c8y/s/uat".into(),
            "".to_string(),
        ))
        .await
        .unwrap();

    let token_smartrest =
        match tokio::time::timeout(Duration::from_secs(10), subscriber.next()).await {
            Ok(Some(msg)) => msg.payload_str().unwrap().to_string(),
            Ok(None) => todo!(),
            Err(_err) => todo!(),
        };

    let mut csv = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(token_smartrest.as_bytes());

    let mut token = SmartRestJwtResponse::new();
    for result in csv.deserialize() {
        token = result.unwrap();
    }

    Ok(token)
}
