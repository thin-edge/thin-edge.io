use crate::mapper::mqtt_config;
use crate::sm_c8y_mapper::{error::*, json_c8y::C8yUpdateSoftwareListResponse, topic::*};
use crate::{component::TEdgeComponent, sm_c8y_mapper::json_c8y::InternalIdResponse};
use async_trait::async_trait;
use c8y_smartrest::{
    smartrest_deserializer::{SmartRestJwtResponse, SmartRestUpdateSoftware},
    smartrest_serializer::{
        SmartRestGetPendingOperations, SmartRestSerializer, SmartRestSetOperationToExecuting,
        SmartRestSetOperationToFailed, SmartRestSetOperationToSuccessful,
        SmartRestSetSupportedOperations,
    },
};
use json_sm::{
    Jsonify, SoftwareListRequest, SoftwareListResponse, SoftwareOperationStatus,
    SoftwareUpdateResponse,
};
use mqtt_client::{Client, MqttClient, MqttClientError, MqttMessageStream, Topic, TopicFilter};
use std::{convert::TryInto, time::Duration};
use tedge_config::{C8yUrlSetting, ConfigSettingAccessorStringExt, DeviceIdSetting, TEdgeConfig};
use tokio::time::Instant;
use tracing::{debug, error, info, instrument};

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

    #[instrument(skip(self), name = "init")]
    async fn init(&mut self) -> Result<(), anyhow::Error> {
        info!("Initialisation");
        while self.c8y_internal_id.is_empty() {
            if let Err(error) = self.try_get_and_set_internal_id().await {
                error!("{:?}", error);

                tokio::time::sleep_until(Instant::now() + Duration::from_secs(300)).await;
                continue;
            };
        }

        Ok(())
    }

    async fn run(&self, mut messages: Box<dyn MqttMessageStream>) -> Result<(), anyhow::Error> {
        info!("Running");
        let () = self.publish_supported_operations().await?;
        let () = self.publish_get_pending_operations().await?;
        let () = self.ask_software_list().await?;

        while let Err(err) = self.subscribe_messages_runtime(&mut messages).await {
            error!("{}", err);
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
                    let () = self
                        .forward_software_request(message.payload_str()?)
                        .await?;
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
            .from_smartrest(smartrest)?
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
            Err(err) => return Err(SMCumulocityMapperError::FromElapsed(err)),
        };

    Ok(SmartRestJwtResponse::try_new(&token_smartrest)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_client::MqttMessageStream;
    use std::sync::Arc;
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
}
