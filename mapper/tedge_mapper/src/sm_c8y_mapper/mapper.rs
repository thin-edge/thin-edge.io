use crate::component::TEdgeComponent;
use crate::mapper::mqtt_config;
use crate::sm_c8y_mapper::{
    error::*, smartrest_deserializer::*, smartrest_serializer::*, topic::*,
};
use async_trait::async_trait;
use json_sm::{
    Jsonify, SoftwareListRequest, SoftwareListResponse, SoftwareOperationStatus,
    SoftwareUpdateResponse,
};
use mqtt_client::{Client, MqttClient, MqttClientError, Topic, TopicFilter};
use std::convert::TryInto;
use tedge_config::TEdgeConfig;
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

        let sm_mapper = CumulocitySoftwareManagement::new(mqtt_client);
        let () = sm_mapper.run().await?;

        Ok(())
    }
}

#[derive(Debug)]
struct CumulocitySoftwareManagement {
    client: Client,
}

impl CumulocitySoftwareManagement {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    async fn run(&self) -> Result<(), anyhow::Error> {
        let () = self.publish_supported_operations().await?;
        let () = self.publish_get_pending_operations().await?;
        let () = self.ask_software_list().await?;

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
                    let () = self.validate_and_publish_software_list(message.payload_str()?).await?;
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

    async fn publish_software_list(
        &self,
        json_response: &str,
    ) -> Result<(), SMCumulocityMapperError> {
        let response = SoftwareListResponse::from_json(json_response)?;

        let topic = OutgoingTopic::SmartRestResponse.to_topic()?;
        let smartrest_response =
            SmartRestSetSoftwareList::from_thin_edge_json(response).to_smartrest()?;
        let () = self.publish(&topic, smartrest_response).await?;

        Ok(())
    }

    async fn validate_and_publish_software_list(
        &self,
        json_response: &str,
    ) -> Result<(), SMCumulocityMapperError> {
        let response = SoftwareListResponse::from_json(json_response)?;

        match response.status() {
            SoftwareOperationStatus::Successful => {
                self.publish_software_list(json_response).await?;
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
                let () = self.publish_software_list(json_response).await?;
            }
            SoftwareOperationStatus::Failed => {
                let smartrest_set_operation =
                    SmartRestSetOperationToFailed::from_thin_edge_json(response)?.to_smartrest()?;
                let () = self.publish(&topic, smartrest_set_operation).await?;
                let () = self.publish_software_list(json_response).await?;
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
}
