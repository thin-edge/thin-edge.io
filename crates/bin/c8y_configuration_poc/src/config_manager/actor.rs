use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResponse;
use crate::file_system_ext::FsWatchEvent;
use crate::mqtt_ext::MqttMessage;
use async_trait::async_trait;
use tedge_actors::adapt;
use tedge_actors::fan_in_message_type;
use tedge_actors::mpsc;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;
use tedge_actors::StreamExt;

use super::config_manager::ConfigManager;

fan_in_message_type!(ConfigInputAndResponse[MqttMessage, FsWatchEvent, C8YRestResponse] : Debug);
fan_in_message_type!(ConfigInput[MqttMessage, FsWatchEvent] : Debug);
fan_in_message_type!(ConfigOutput[MqttMessage, C8YRestRequest] : Debug);

pub struct ConfigManagerActor {
    pub config_manager: ConfigManager,
}

#[async_trait]
impl Actor for ConfigManagerActor {
    type MessageBox = ConfigManagerMessageBox;

    async fn run(mut self, mut messages: Self::MessageBox) -> Result<(), ChannelError> {
        while let Some(event) = messages.events.next().await {
            match event {
                ConfigInput::MqttMessage(message) => {
                    self.config_manager
                        .process_mqtt_message(message)
                        .await
                        .unwrap();
                }
                ConfigInput::FsWatchEvent(event) => {
                    self.config_manager
                        .process_file_watch_events(event)
                        .await
                        .unwrap();
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
