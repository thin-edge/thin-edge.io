use std::path::PathBuf;
use tedge_actors::{Actor, adapt, Address, ChannelError, fan_in_message_type, Mailbox, new_mailbox, Recipient, RuntimeError, RuntimeHandle};
use crate::{file_system_ext, http_ext, mqtt_ext};
use crate::file_system_ext::*;
use crate::http_ext::*;
use crate::mqtt_ext::*;
use async_trait::async_trait;

pub struct ConfigConfig {
    pub mqtt_conf: MqttConfig,
    pub http_conf: HttpConfig,
    pub config_dir: PathBuf,
}

impl ConfigConfig {
    /// What are the steps to create a new Actor?
    ///
    /// 1. Create the actor mailbox with the associated address.
    ///    * Most actors have a single mailbox,
    ///    * but some actors might have several mailboxes each with a specific address,
    ///      notably, for waiting for responses on dedicated channels.
    ///    * The mailbox can be private, but the address must be public
    ///      to be given to other actors.
    /// 1. Create the actor peer handlers.
    /// 1. Create the initial state from a config.
    /// 1. Spawn the process, returning a handle to send messages .
    pub async fn spawn_actor(self, runtime: &mut RuntimeHandle) -> Result<ConfigManagerAddress,RuntimeError> {
        let actor = ConfigActor {
            config_dir: self.config_dir.clone(),
        };
        let watcher_config = file_system_ext::WatcherConfig {
            directory: self.config_dir,
        };

        let (mailbox, address)= new_config_mailbox();
        let mqtt_con = mqtt_ext::new_connection(runtime, self.mqtt_conf, address.events.as_recipient());
        let http_con = http_ext::new_connection(runtime, self.http_conf, address.http_responses.as_recipient());
        let file_watcher = file_system_ext::new_watcher(runtime, watcher_config, address.events.as_recipient());

        let peers = ConfigManagerPeers {
            file_watcher,
            http_con,
            mqtt_con
        };

        runtime.run(actor, mailbox, peers).await?;
        Ok(address)
    }
}

fan_in_message_type!(ConfigInputAndResponse[MqttMessage, FileEvent, HttpResponse] : Clone , Debug);
fan_in_message_type!(ConfigInput[MqttMessage, FileEvent] : Clone , Debug);
fan_in_message_type!(ConfigOutput[MqttMessage, HttpRequest, FileRequest] : Clone , Debug);

struct ConfigActor {
    config_dir: PathBuf,
}

#[async_trait]
impl Actor for ConfigActor {
    type Input = ConfigInputAndResponse;
    type Output = ConfigOutput;
    type Mailbox = ConfigManagerMailbox;
    type Peers = ConfigManagerPeers;

    async fn run(self, messages: Self::Mailbox, peers: Self::Peers) -> Result<(), ChannelError> {
        todo!()
    }
}

struct ConfigManagerMailbox {
    events: Mailbox<ConfigInput>,
    http_responses: Mailbox<HttpResponse>,
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
    http_responses: Address<HttpResponse>,
}

pub fn new_config_mailbox() -> (ConfigManagerMailbox, ConfigManagerAddress) {
    let (events_mailbox, events_address) = new_mailbox(10);
    let (http_mailbox, http_address) = new_mailbox(10);
    (ConfigManagerMailbox {
        events: events_mailbox,
        http_responses: http_mailbox,
    }, ConfigManagerAddress {
        events: events_address,
        http_responses: http_address,
    })
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




