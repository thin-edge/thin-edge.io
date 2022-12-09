use crate::file_system_ext::{FileEvent, FileRequest};
use crate::mqtt_ext::MqttMessage;
use async_trait::async_trait;
use tedge_actors::{
    adapt, fan_in_message_type, new_mailbox, Actor, Address, ChannelError, DynSender, Mailbox,
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
        event: FileEvent,
        messages: &mut ConfigManagerMailbox,
        peers: &mut ConfigManagerPeers,
    ) -> Result<(), ChannelError> {
        todo!()
    }

    pub async fn process_mqtt_message(
        &mut self,
        message: MqttMessage,
        messages: &mut ConfigManagerMailbox,
        peers: &mut ConfigManagerPeers,
    ) -> Result<(), ChannelError> {
        // ..
        let request = todo!();
        let response = ConfigManagerActor::send_http_request(messages, peers, request).await?;
        // ..
        Ok(())
    }

    async fn send_http_request(
        messages: &mut ConfigManagerMailbox,
        peers: &mut ConfigManagerPeers,
        request: HttpRequest,
    ) -> Result<HttpResult, ChannelError> {
        peers.http_con.send(request).await?;
        if let Some(response) = messages.http_responses.next().await {
            Ok(response)
        } else {
            Err(ChannelError::ReceiveError())
        }
    }
}

#[async_trait]
impl Actor for ConfigManagerActor {
    type Input = ConfigInputAndResponse;
    type Output = ConfigOutput;
    type Mailbox = ConfigManagerMailbox;
    type Peers = ConfigManagerPeers;

    async fn run(
        mut self,
        mut messages: Self::Mailbox,
        mut peers: Self::Peers,
    ) -> Result<(), ChannelError> {
        while let Some(event) = messages.events.next().await {
            match event {
                ConfigInput::MqttMessage(message) => {
                    self.process_mqtt_message(message, &mut messages, &mut peers)
                        .await?;
                }
                ConfigInput::FileEvent(event) => {
                    self.process_file_event(event, &mut messages, &mut peers)
                        .await?;
                }
            }
        }
        Ok(())
    }
}

pub struct ConfigManagerMailbox {
    pub events: Mailbox<ConfigInput>,
    pub http_responses: Mailbox<HttpResult>,
}

impl From<Mailbox<ConfigInputAndResponse>> for ConfigManagerMailbox {
    fn from(_: Mailbox<ConfigInputAndResponse>) -> Self {
        todo!()
    }
}

// Is this struct useful?
//
// Could be useful for peers to peek the appropriate address.
// There is no such peers in this plugin, but it could be in a larger plugin
// that includes all the C8Y features
pub struct ConfigManagerAddress {
    pub events: Address<ConfigInput>,
    pub http_responses: Address<HttpResult>,
}

pub fn new_config_mailbox() -> (ConfigManagerMailbox, ConfigManagerAddress) {
    let (events_mailbox, events_address) = new_mailbox(10);
    let (http_mailbox, http_address) = new_mailbox(10);
    (
        ConfigManagerMailbox {
            events: events_mailbox,
            http_responses: http_mailbox,
        },
        ConfigManagerAddress {
            events: events_address,
            http_responses: http_address,
        },
    )
}

pub struct ConfigManagerPeers {
    file_watcher: DynSender<FileRequest>,
    http_con: DynSender<HttpRequest>,
    mqtt_con: DynSender<MqttMessage>,
}

impl ConfigManagerPeers {
    pub fn new(
        file_watcher: DynSender<FileRequest>,
        http_con: DynSender<HttpRequest>,
        mqtt_con: DynSender<MqttMessage>,
    ) -> ConfigManagerPeers {
        ConfigManagerPeers {
            file_watcher,
            http_con,
            mqtt_con,
        }
    }
}
impl From<DynSender<ConfigOutput>> for ConfigManagerPeers {
    fn from(recipient: DynSender<ConfigOutput>) -> Self {
        ConfigManagerPeers {
            file_watcher: adapt(&recipient),
            http_con: adapt(&recipient),
            mqtt_con: adapt(&recipient),
        }
    }
}
