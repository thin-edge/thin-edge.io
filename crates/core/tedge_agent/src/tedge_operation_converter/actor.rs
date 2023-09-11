use crate::software_manager::actor::SoftwareRequest;
use crate::software_manager::actor::SoftwareResponse;
use crate::tedge_operation_converter::error::TedgeOperationConverterError;
use async_trait::async_trait;
use log::error;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::LoggingReceiver;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::Jsonify;
use tedge_api::RestartOperationRequest;
use tedge_api::RestartOperationResponse;
use tedge_api::SoftwareListRequest;
use tedge_api::SoftwareListResponse;
use tedge_api::SoftwareUpdateRequest;
use tedge_api::SoftwareUpdateResponse;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;

fan_in_message_type!(AgentInput[MqttMessage, SoftwareResponse, RestartOperationResponse] : Debug);

pub struct TedgeOperationConverterActor {
    input_receiver: LoggingReceiver<AgentInput>,
    software_sender: LoggingSender<SoftwareRequest>,
    restart_sender: LoggingSender<RestartOperationRequest>,
    mqtt_publisher: LoggingSender<MqttMessage>,
}

#[async_trait]
impl Actor for TedgeOperationConverterActor {
    fn name(&self) -> &str {
        "TedgeOperationConverter"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        while let Some(input) = self.input_receiver.recv().await {
            match input {
                AgentInput::MqttMessage(message) => {
                    self.process_mqtt_message(message).await?;
                }
                AgentInput::SoftwareResponse(SoftwareResponse::SoftwareListResponse(res)) => {
                    self.process_software_list_response(res).await?;
                }
                AgentInput::SoftwareResponse(SoftwareResponse::SoftwareUpdateResponse(res)) => {
                    self.process_software_update_response(res).await?;
                }
                AgentInput::RestartOperationResponse(res) => {
                    self.process_restart_response(res).await?;
                }
            }
        }
        Ok(())
    }
}

impl TedgeOperationConverterActor {
    pub fn new(
        input_receiver: LoggingReceiver<AgentInput>,
        software_sender: LoggingSender<SoftwareRequest>,
        restart_sender: LoggingSender<RestartOperationRequest>,
        mqtt_publisher: LoggingSender<MqttMessage>,
    ) -> Self {
        Self {
            input_receiver,
            software_sender,
            restart_sender,
            mqtt_publisher,
        }
    }

    async fn process_mqtt_message(
        &mut self,
        message: MqttMessage,
    ) -> Result<(), TedgeOperationConverterError> {
        match message.topic.name.as_str() {
            "tedge/commands/req/software/list" => {
                match SoftwareListRequest::from_slice(message.payload_bytes()) {
                    Ok(request) => {
                        self.software_sender
                            .send(SoftwareRequest::SoftwareListRequest(request))
                            .await?;
                    }
                    Err(err) => error!("Incorrect software list request payload: {err}"),
                }
            }
            "tedge/commands/req/software/update" => {
                match SoftwareUpdateRequest::from_slice(message.payload_bytes()) {
                    Ok(request) => {
                        self.software_sender
                            .send(SoftwareRequest::SoftwareUpdateRequest(request))
                            .await?;
                    }
                    Err(err) => error!("Incorrect software update request payload: {err}"),
                }
            }
            _ => {
                // Not a tedge/commands !
            }
        }

        let mqtt_schema = MqttSchema::default(); // FIXME use the correct root suffix
        match mqtt_schema.entity_channel_of(&message.topic) {
            Ok((
                _target,
                Channel::Command {
                    operation: OperationType::Restart,
                    ..
                },
            )) => {
                // FIXME extract the command id from the topic, not the payload
                match RestartOperationRequest::from_slice(message.payload_bytes()) {
                    Ok(request) => {
                        self.restart_sender.send(request).await?;
                    }
                    Err(err) => error!("Incorrect restart request payload: {err}"),
                }
            }
            _ => {
                log::error!("Unknown command channel: {}", message.topic.name);
            }
        }
        Ok(())
    }

    async fn process_software_list_response(
        &mut self,
        response: SoftwareListResponse,
    ) -> Result<(), TedgeOperationConverterError> {
        let message = MqttMessage::new(
            &Topic::new_unchecked("tedge/commands/res/software/list"),
            response.to_bytes()?,
        );
        self.mqtt_publisher.send(message).await?;
        Ok(())
    }

    async fn process_software_update_response(
        &mut self,
        response: SoftwareUpdateResponse,
    ) -> Result<(), TedgeOperationConverterError> {
        let message = MqttMessage::new(
            &Topic::new_unchecked("tedge/commands/res/software/update"),
            response.to_bytes()?,
        );
        self.mqtt_publisher.send(message).await?;
        Ok(())
    }

    async fn process_restart_response(
        &mut self,
        response: RestartOperationResponse,
    ) -> Result<(), TedgeOperationConverterError> {
        let message = MqttMessage::new(&response.topic(), response.to_bytes()?);
        self.mqtt_publisher.send(message).await?;
        Ok(())
    }
}
