use crate::file_system_ext::{FileEvent, FileRequest};
use crate::mqtt_ext::MqttMessage;
use async_trait::async_trait;
use tedge_actors::{
    fan_in_message_type, mpsc, Actor, ChannelError, DynSender, MessageBox, StreamExt,
};
use tedge_http_ext::{HttpError, HttpRequest, HttpResponse};

type HttpResult = Result<HttpResponse, HttpError>;

fan_in_message_type!(ConfigInputAndResponse[MqttMessage, FileEvent, HttpResult] : Debug);
fan_in_message_type!(ConfigInput[MqttMessage, FileEvent] : Debug);
fan_in_message_type!(ConfigOutput[MqttMessage, HttpRequest, FileRequest] : Debug);

pub struct ConfigManagerActor {}

impl ConfigManagerActor {
    pub async fn process_file_event(
        &mut self,
        _event: FileEvent,
        _messages: &mut ConfigManagerMessageBox,
    ) -> Result<(), ChannelError> {
        todo!()
    }

    pub async fn process_mqtt_message(
        &mut self,
        _message: MqttMessage,
        messages: &mut ConfigManagerMessageBox,
    ) -> Result<(), ChannelError> {
        // ..
        let request = HttpRequest::new(Default::default(), "https://my-tenant.c8y.io")
            .expect("well formed url");
        let _response = messages.send_http_request(request).await?;
        // ..
        Ok(())
    }
}

#[async_trait]
impl Actor for ConfigManagerActor {
    type MessageBox = ConfigManagerMessageBox;

    async fn run(mut self, mut messages: Self::MessageBox) -> Result<(), ChannelError> {
        while let Some(event) = messages.events.next().await {
            match event {
                ConfigInput::MqttMessage(message) => {
                    self.process_mqtt_message(message, &mut messages).await?;
                }
                ConfigInput::FileEvent(event) => {
                    self.process_file_event(event, &mut messages).await?;
                }
            }
        }
        Ok(())
    }
}

pub struct ConfigManagerMessageBox {
    pub events: mpsc::Receiver<ConfigInput>,
    pub http_responses: mpsc::Receiver<HttpResult>,
    pub file_watcher: DynSender<FileRequest>,
    pub http_con: DynSender<HttpRequest>,
    pub mqtt_con: DynSender<MqttMessage>,
}

impl ConfigManagerMessageBox {
    pub fn new(
        events: mpsc::Receiver<ConfigInput>,
        http_responses: mpsc::Receiver<HttpResult>,
        file_watcher: DynSender<FileRequest>,
        http_con: DynSender<HttpRequest>,
        mqtt_con: DynSender<MqttMessage>,
    ) -> ConfigManagerMessageBox {
        ConfigManagerMessageBox {
            events,
            http_responses,
            file_watcher,
            http_con,
            mqtt_con,
        }
    }

    async fn send_http_request(
        &mut self,
        request: HttpRequest,
    ) -> Result<HttpResult, ChannelError> {
        self.http_con.send(request).await?;
        if let Some(response) = self.http_responses.next().await {
            Ok(response)
        } else {
            Err(ChannelError::ReceiveError())
        }
    }
}

impl MessageBox for ConfigManagerMessageBox {}
