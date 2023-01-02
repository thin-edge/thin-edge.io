use std::path::Path;

use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResponse;
use crate::file_system_ext::FsWatchEvent;
use crate::mqtt_ext::MqttMessage;
use async_trait::async_trait;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_api::OffsetDateTime;
use mqtt_channel::Message;
use tedge_actors::adapt;
use tedge_actors::fan_in_message_type;
use tedge_actors::mpsc;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;
use tedge_actors::StreamExt;

fan_in_message_type!(ConfigInputAndResponse[MqttMessage, FsWatchEvent, C8YRestResponse] : Debug);
fan_in_message_type!(ConfigInput[MqttMessage, FsWatchEvent] : Debug);
fan_in_message_type!(ConfigOutput[MqttMessage, C8YRestRequest] : Debug);

pub struct ConfigManagerActor {}

impl ConfigManagerActor {
    pub async fn process_file_event(
        &mut self,
        event: FsWatchEvent,
        messages: &mut ConfigManagerMessageBox,
    ) -> Result<(), ChannelError> {
        let path = match event {
            FsWatchEvent::Modified(path) => path,
            FsWatchEvent::FileDeleted(path) => path,
            FsWatchEvent::FileCreated(path) => path,
            FsWatchEvent::DirectoryDeleted(_) => return Ok(()),
            FsWatchEvent::DirectoryCreated(_) => return Ok(()),
        };

        let mqtt_message = Self::supported_operations_message(path.as_path());
        messages.send(ConfigOutput::MqttMessage(mqtt_message)).await
    }

    pub fn supported_operations_message(_path: &Path) -> MqttMessage {
        let topic = C8yTopic::SmartRestResponse.to_topic().expect("Infallible");
        Message::new(&topic, "114")
    }

    pub async fn process_mqtt_message(
        &mut self,
        _message: MqttMessage,
        messages: &mut ConfigManagerMessageBox,
    ) -> Result<(), ChannelError> {
        // ..
        let request = C8yCreateEvent::new(
            None,
            "".to_string(),
            OffsetDateTime::now_utc(),
            Default::default(),
            Default::default(),
        );
        let _response = messages
            .send_http_request(C8YRestRequest::C8yCreateEvent(request))
            .await?;
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
                ConfigInput::FsWatchEvent(event) => {
                    self.process_file_event(event, &mut messages).await?;
                }
            }
        }
        Ok(())
    }
}

pub struct ConfigManagerMessageBox {
    pub events: mpsc::Receiver<ConfigInput>,
    pub http_responses: mpsc::Receiver<C8YRestResponse>,
    pub http_requests: DynSender<C8YRestRequest>,
    pub mqtt_requests: DynSender<MqttMessage>,
}

impl ConfigManagerMessageBox {
    pub fn new(
        events: mpsc::Receiver<ConfigInput>,
        http_responses: mpsc::Receiver<C8YRestResponse>,
        http_con: DynSender<C8YRestRequest>,
        mqtt_con: DynSender<MqttMessage>,
    ) -> ConfigManagerMessageBox {
        ConfigManagerMessageBox {
            events,
            http_responses,
            http_requests: http_con,
            mqtt_requests: mqtt_con,
        }
    }

    async fn send_http_request(
        &mut self,
        request: C8YRestRequest,
    ) -> Result<C8YRestResponse, ChannelError> {
        self.http_requests.send(request).await?;
        if let Some(response) = self.http_responses.next().await {
            Ok(response)
        } else {
            Err(ChannelError::ReceiveError())
        }
    }
}

#[async_trait]
impl MessageBox for ConfigManagerMessageBox {
    type Input = ConfigInputAndResponse;
    type Output = ConfigOutput;

    async fn recv(&mut self) -> Option<Self::Input> {
        tokio::select! {
            Some(message) = self.events.next() => {
                match message {
                    ConfigInput::MqttMessage(message) => {
                        Some(ConfigInputAndResponse::MqttMessage(message))
                    },
                    ConfigInput::FsWatchEvent(message) => {
                        Some(ConfigInputAndResponse::FsWatchEvent(message))
                    }
                }
            },
            Some(message) = self.http_responses.next() => {
                Some(ConfigInputAndResponse::C8YRestResponse(message))
            },
            else => None,
        }
    }

    async fn send(&mut self, message: Self::Output) -> Result<(), ChannelError> {
        match message {
            ConfigOutput::MqttMessage(msg) => self.mqtt_requests.send(msg).await,
            ConfigOutput::C8YRestRequest(msg) => self.http_requests.send(msg).await,
        }
    }

    fn new_box(capacity: usize, output: DynSender<Self::Output>) -> (DynSender<Self::Input>, Self) {
        let (events_sender, events_receiver) = mpsc::channel(capacity);
        let (http_responses_sender, http_responses_receiver) = mpsc::channel(1);
        let input_sender = FanOutSender {
            events_sender,
            http_responses_sender,
        };
        let message_box = ConfigManagerMessageBox {
            events: events_receiver,
            http_responses: http_responses_receiver,
            http_requests: adapt(&output.clone()),
            mqtt_requests: adapt(&output.clone()),
        };
        (input_sender.into(), message_box)
    }
}

// One should be able to have a macro to generate this fan-out sender type
#[derive(Clone)]
struct FanOutSender {
    events_sender: mpsc::Sender<ConfigInput>,
    http_responses_sender: mpsc::Sender<C8YRestResponse>,
}

#[async_trait]
impl tedge_actors::Sender<ConfigInputAndResponse> for FanOutSender {
    async fn send(&mut self, message: ConfigInputAndResponse) -> Result<(), ChannelError> {
        match message {
            ConfigInputAndResponse::MqttMessage(msg) => self.events_sender.send(msg).await,
            ConfigInputAndResponse::FsWatchEvent(msg) => self.events_sender.send(msg).await,
            ConfigInputAndResponse::C8YRestResponse(msg) => {
                self.http_responses_sender.send(msg).await
            }
        }
    }

    fn sender_clone(&self) -> DynSender<ConfigInputAndResponse> {
        Box::new(self.clone())
    }
}

impl From<FanOutSender> for tedge_actors::DynSender<ConfigInputAndResponse> {
    fn from(sender: FanOutSender) -> Self {
        Box::new(sender)
    }
}
