use crate::file_system_ext::*;
use crate::mqtt_ext::*;
use crate::{file_system_ext, mqtt_ext};
use async_trait::async_trait;
use std::path::PathBuf;
use tedge_actors::{
    adapt, fan_in_message_type, new_mailbox, Actor, ActorInstance, Address, ChannelError, Mailbox,
    Recipient, RuntimeError, RuntimeHandle,
};
use tedge_http_ext::*;

/// Configuration of the Configuration Manager
#[derive(Clone, Debug)]
pub struct ConfigConfigManager {
    pub mqtt_conf: MqttConfig,
    pub http_conf: HttpConfig,
    pub config_dir: PathBuf,
}

/// An instance of the config manager
///
/// This is an actor builder.
pub struct ConfigManager {
    config: ConfigConfigManager,
    mailbox: ConfigManagerMailbox,
    address: ConfigManagerAddress,
    http_con: Option<Recipient<HttpRequest>>,
}

impl ConfigManager {
    pub fn new(config: ConfigConfigManager) -> ConfigManager {
        let (mailbox, address) = new_config_mailbox();

        ConfigManager {
            config,
            mailbox,
            address,
            http_con: None,
        }
    }

    /// Connect this config manager instance to some http connection provider
    ///
    /// TODO: the `http` actor should not be a concrete implementation
    ///       but an instance that consumes and produces messages of the expected type.
    pub fn with_http_connection(&mut self, http: &mut HttpActorInstance) {
        let http_con = http.add_client(self.address.http_responses.as_recipient());
        self.http_con = Some(http_con);
    }
}

#[async_trait]
impl ActorInstance for ConfigManager {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let actor = ConfigManagerActor {
            config: self.config.clone(),
        };

        let watcher_config = file_system_ext::WatcherConfig {
            directory: self.config.config_dir,
        };

        let mqtt_con = mqtt_ext::new_connection(
            runtime,
            self.config.mqtt_conf,
            self.address.events.as_recipient(),
        )
        .await?;

        let file_watcher = file_system_ext::new_watcher(
            runtime,
            watcher_config,
            self.address.events.as_recipient(),
        )
        .await?;

        let peers = ConfigManagerPeers {
            file_watcher,
            http_con: self.http_con.expect("Missing http connection"), // TODO: add error handling
            mqtt_con,
        };

        runtime.run(actor, self.mailbox, peers).await?;
        Ok(())
    }
}

type HttpResult = Result<HttpResponse, HttpError>;

fan_in_message_type!(ConfigInputAndResponse[MqttMessage, FileEvent, HttpResult] : Debug);
fan_in_message_type!(ConfigInput[MqttMessage, FileEvent] : Debug);
fan_in_message_type!(ConfigOutput[MqttMessage, HttpRequest, FileRequest] : Debug);

struct ConfigManagerActor {
    config: ConfigConfigManager,
}

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

struct ConfigManagerMailbox {
    events: Mailbox<ConfigInput>,
    http_responses: Mailbox<HttpResult>,
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
    events: Address<ConfigInput>,
    http_responses: Address<HttpResult>,
}

fn new_config_mailbox() -> (ConfigManagerMailbox, ConfigManagerAddress) {
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

struct ConfigManagerPeers {
    file_watcher: Recipient<FileRequest>,
    http_con: Recipient<HttpRequest>,
    mqtt_con: Recipient<MqttMessage>,
}

impl From<Recipient<ConfigOutput>> for ConfigManagerPeers {
    fn from(recipient: Recipient<ConfigOutput>) -> Self {
        ConfigManagerPeers {
            file_watcher: adapt(&recipient),
            http_con: adapt(&recipient),
            mqtt_con: adapt(&recipient),
        }
    }
}
