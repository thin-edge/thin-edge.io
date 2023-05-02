use crate::mqtt_operation_converter::error::MqttRequestConverterError;
use async_trait::async_trait;
use mqtt_channel::Topic;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::DynSender;
use tedge_actors::LoggingReceiver;
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

fan_in_message_type!(AgentInput[MqttMessage, SoftwareListResponse, SoftwareUpdateResponse, RestartOperationResponse] : Debug);

pub struct MqttOperationConverterActor {
    input_receiver: LoggingReceiver<AgentInput>,
    software_list_sender: DynSender<SoftwareListRequest>,
    software_update_sender: DynSender<SoftwareUpdateRequest>,
    restart_sender: DynSender<RestartOperationRequest>,
    mqtt_publisher: DynSender<MqttMessage>,
}

#[async_trait]
impl Actor for MqttOperationConverterActor {
    fn name(&self) -> &str {
        "MqttOperationConverter"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        while let Some(input) = self.input_receiver.recv().await {
            match input {
                AgentInput::MqttMessage(message) => {
                    self.process_mqtt_message(message).await?;
                }
                AgentInput::SoftwareListResponse(res) => {
                    self.process_software_list_response(res).await?;
                }
                AgentInput::SoftwareUpdateResponse(res) => {
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

impl MqttOperationConverterActor {
    pub fn new(
        input_receiver: LoggingReceiver<AgentInput>,
        software_list_sender: DynSender<SoftwareListRequest>,
        software_update_sender: DynSender<SoftwareUpdateRequest>,
        restart_sender: DynSender<RestartOperationRequest>,
        mqtt_publisher: DynSender<MqttMessage>,
    ) -> Self {
        Self {
            input_receiver,
            software_list_sender,
            software_update_sender,
            restart_sender,
            mqtt_publisher,
        }
    }

    async fn process_mqtt_message(
        &mut self,
        message: MqttMessage,
    ) -> Result<(), MqttRequestConverterError> {
        // TODO: Update after Albin's mapper PR.
        match message.topic.name.as_str() {
            "tedge/commands/req/software/list" => {
                let request = SoftwareListRequest::from_slice(message.payload_bytes())?;
                self.software_list_sender.send(request).await?;
            }
            "tedge/commands/req/software/update" => {
                let request = SoftwareUpdateRequest::from_slice(message.payload_bytes())?;
                self.software_update_sender.send(request).await?;
            }
            "tedge/commands/req/control/restart" => {
                let request = RestartOperationRequest::from_slice(message.payload_bytes())?;
                self.restart_sender.send(request).await?;
            }
            _ => unimplemented!(),
        }
        Ok(())
    }

    async fn process_software_list_response(
        &mut self,
        response: SoftwareListResponse,
    ) -> Result<(), MqttRequestConverterError> {
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
    ) -> Result<(), MqttRequestConverterError> {
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
    ) -> Result<(), MqttRequestConverterError> {
        let message = MqttMessage::new(
            &Topic::new_unchecked("tedge/commands/res/control/restart"),
            response.to_bytes()?,
        );
        self.mqtt_publisher.send(message).await?;
        Ok(())
    }
}
