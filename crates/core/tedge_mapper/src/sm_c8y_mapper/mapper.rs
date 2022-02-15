use crate::component::TEdgeComponent;

use agent_interface::{
    topic::*, Jsonify, OperationStatus, RestartOperationRequest, RestartOperationResponse,
    SoftwareListRequest, SoftwareListResponse, SoftwareUpdateResponse,
};
use async_trait::async_trait;
use c8y_api::{
    http_proxy::{C8YHttpProxy, JwtAuthHttpProxy},
    json_c8y::C8yUpdateSoftwareListResponse,
};
use c8y_smartrest::smartrest_deserializer::SmartRestRestartRequest;
use c8y_smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_smartrest::{
    error::{SMCumulocityMapperError, SmartRestDeserializerError},
    operations::Operations,
    smartrest_deserializer::SmartRestUpdateSoftware,
    smartrest_serializer::{
        SmartRestGetPendingOperations, SmartRestSerializer, SmartRestSetOperationToExecuting,
        SmartRestSetOperationToFailed, SmartRestSetOperationToSuccessful,
        SmartRestSetSupportedLogType,
    },
    topic::*,
};
use download::{Auth, DownloadInfo};
use mqtt_channel::{Config, Connection, MqttError, SinkExt, StreamExt, Topic, TopicFilter};
use std::{convert::TryInto, process::Stdio};
use tedge_config::{ConfigSettingAccessor, MqttPortSetting, TEdgeConfig};
use tracing::{debug, error, info, instrument};

const SM_MAPPER: &str = "SM-C8Y-Mapper";
const SM_MAPPER_JWT_TOKEN_SESSION_NAME: &str = "SM-C8Y-Mapper-JWT-Token";

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
        let http_proxy =
            JwtAuthHttpProxy::try_new(&tedge_config, SM_MAPPER_JWT_TOKEN_SESSION_NAME).await?;
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
        let request = SoftwareListRequest::default();
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
    async fn forward_software_request(
        &mut self,
        smartrest: &str,
    ) -> Result<(), SMCumulocityMapperError> {
        let topic = Topic::new(RequestTopic::SoftwareUpdateRequest.as_str())?;
        let update_software = SmartRestUpdateSoftware::new();
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

        let request = RestartOperationRequest::default();
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
