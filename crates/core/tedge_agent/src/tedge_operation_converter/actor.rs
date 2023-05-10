use crate::software_manager::actor::SoftwareRequest;
use crate::software_manager::actor::SoftwareResponse;
use crate::tedge_operation_converter::error::TedgeOperationConverterError;
use async_trait::async_trait;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::LoggingReceiver;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
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
                let request = SoftwareListRequest::from_slice(message.payload_bytes())?;
                self.software_sender
                    .send(SoftwareRequest::SoftwareListRequest(request))
                    .await?;
            }
            "tedge/commands/req/software/update" => {
                let request = SoftwareUpdateRequest::from_slice(message.payload_bytes())?;
                self.software_sender
                    .send(SoftwareRequest::SoftwareUpdateRequest(request))
                    .await?;
            }
            "tedge/commands/req/control/restart" => {
                let request = RestartOperationRequest::from_slice(message.payload_bytes())?;
                self.restart_sender.send(request).await?;
            }
            _ => unreachable!(),
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
        let message = MqttMessage::new(
            &Topic::new_unchecked("tedge/commands/res/control/restart"),
            response.to_bytes()?,
        );
        self.mqtt_publisher.send(message).await?;
        Ok(())
    }
}
