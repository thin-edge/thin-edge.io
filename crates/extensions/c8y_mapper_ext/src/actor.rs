use super::config::C8yMapperConfig;
use super::converter::CumulocityConverter;
use super::dynamic_discovery::process_inotify_events;
use crate::converter::Converter;
use async_trait::async_trait;
use c8y_api::smartrest::topic::SMARTREST_PUBLISH_TOPIC;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
use std::path::PathBuf;
use std::time::Duration;
use tedge_actors::adapt;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sender;
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::entity_store;
use tedge_api::EntityStore;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::Message;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::FileError;
use tracing::error;

const MQTT_ROOT: &str = "te";
const SYNC_WINDOW: Duration = Duration::from_secs(3);

pub type SyncStart = SetTimeout<()>;
pub type SyncComplete = Timeout<()>;

fan_in_message_type!(C8yMapperInput[MqttMessage, FsWatchEvent, SyncComplete] : Debug);
type C8yMapperOutput = MqttMessage;

pub struct C8yMapperActor {
    converter: CumulocityConverter,
    messages: SimpleMessageBox<C8yMapperInput, C8yMapperOutput>,
    mqtt_publisher: LoggingSender<MqttMessage>,
    timer_sender: LoggingSender<SyncStart>,
    entity_store: EntityStore,
}

#[async_trait]
impl Actor for C8yMapperActor {
    fn name(&self) -> &str {
        "CumulocityMapper"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        let init_messages = self.converter.init_messages();
        for init_message in init_messages.into_iter() {
            let _ = self.mqtt_publisher.send(init_message).await?;
        }

        // Start the sync phase
        self.timer_sender
            .send(SyncStart::new(SYNC_WINDOW, ()))
            .await?;

        while let Some(event) = self.messages.recv().await {
            match event {
                C8yMapperInput::MqttMessage(message) => {
                    self.process_mqtt_message(message).await?;
                }
                C8yMapperInput::FsWatchEvent(event) => {
                    self.process_file_watch_event(event).await?;
                }
                C8yMapperInput::SyncComplete(_) => {
                    self.process_sync_timeout().await?;
                }
            }
        }
        Ok(())
    }
}

impl C8yMapperActor {
    pub fn new(
        converter: CumulocityConverter,
        messages: SimpleMessageBox<C8yMapperInput, C8yMapperOutput>,
        mqtt_publisher: LoggingSender<MqttMessage>,
        timer_sender: LoggingSender<SyncStart>,
        device_id: String,
    ) -> Self {
        let main_device = entity_store::EntityRegistrationMessage::main_device(device_id);
        Self {
            converter,
            messages,
            mqtt_publisher,
            timer_sender,
            entity_store: EntityStore::with_main_device(main_device).unwrap(),
        }
    }

    async fn process_mqtt_message(&mut self, message: MqttMessage) -> Result<(), RuntimeError> {
        if let Some(entity_id) = entity_store::entity_mqtt_id(&message.topic) {
            if is_entity_register_message(&message) {
                if let Err(e) = self
                    .entity_store
                    .update(message.clone().try_into().unwrap())
                {
                    error!("Could not update device registration: {e}");
                }
            } else {
                // if device is unregistered register using auto-registration
                if self.entity_store.get(entity_id).is_none() {
                    let register_messages = match self.auto_register_entity(entity_id) {
                        Ok(register_messages) => register_messages,
                        Err(e) => {
                            error!("Could not update device registration: {e}");
                            vec![]
                        }
                    };

                    for msg in register_messages {
                        let _ = self.mqtt_publisher.send(msg).await;
                    }
                }
            }
        }

        let converted_messages = self.converter.convert(&message).await;

        for converted_message in converted_messages.into_iter() {
            let _ = self.mqtt_publisher.send(converted_message).await;
        }

        Ok(())
    }

    /// Performs auto-registration process for an entity under a given
    /// identifier.
    ///
    /// If an entity is a service, its device is also auto-registered if it's
    /// not already registered.
    ///
    /// It returns MQTT register messages for the given entities to be published
    /// by the mapper, so other components can also be aware of a new device
    /// being registered.
    fn auto_register_entity(
        &mut self,
        entity_id: &str,
    ) -> Result<Vec<Message>, entity_store::Error> {
        let mut register_messages = vec![];
        let (device_id, service_id) = match entity_id.split('/').collect::<Vec<&str>>()[..] {
            ["device", device_id, "service", service_id, ..] => (device_id, Some(service_id)),
            ["device", device_id, "", ""] => (device_id, None),
            _ => return Ok(register_messages),
        };

        // register device if not registered
        let device_topic = format!("device/{device_id}//");
        if self.entity_store.get(&device_topic).is_none() {
            let device_register_payload = r#"{ "@type": "child-device" }"#.to_string();
            let device_register_message = Message::new(
                &Topic::new(&device_topic).unwrap(),
                device_register_payload.clone(),
            )
            .with_retain();
            register_messages.push(device_register_message.clone());
            self.entity_store
                .update(device_register_message.try_into().unwrap())?;
        }

        // register service itself
        if let Some(service_id) = service_id {
            let service_topic = format!("{MQTT_ROOT}/device/{device_id}/service/{service_id}");
            let service_register_payload = r#"{"@type": "service", "type": "systemd"}"#.to_string();
            let service_register_message = Message::new(
                &Topic::new(&service_topic).unwrap(),
                service_register_payload.clone(),
            )
            .with_retain();
            register_messages.push(service_register_message.clone());
            self.entity_store
                .update(service_register_message.try_into().unwrap())?;
        }

        Ok(register_messages)
    }

    /// Registers the entity under a given MQTT topic.
    ///
    /// If a given entity was registered previously, the function will do
    /// nothing. Otherwise it will save registration data to memory, free to be
    /// queried by other components.
    // fn register_entity(&mut self, topic: String, payload: String) {
    //     self.entity_store.entry(&topic).or_insert(payload);
    // }

    async fn process_file_watch_event(
        &mut self,
        file_event: FsWatchEvent,
    ) -> Result<(), RuntimeError> {
        match file_event.clone() {
            FsWatchEvent::DirectoryCreated(path) => {
                if let Some(directory_name) = path.file_name() {
                    let child_id = directory_name.to_string_lossy().to_string();
                    let message = Message::new(
                        &Topic::new_unchecked(SMARTREST_PUBLISH_TOPIC),
                        format!("101,{child_id},{child_id},thin-edge.io-child"),
                    );
                    self.mqtt_publisher.send(message).await?;
                }
            }
            FsWatchEvent::FileCreated(path)
            | FsWatchEvent::FileDeleted(path)
            | FsWatchEvent::Modified(path)
            | FsWatchEvent::DirectoryDeleted(path) => {
                match process_inotify_events(&path, file_event) {
                    Ok(Some(discovered_ops)) => {
                        self.mqtt_publisher
                            .send(
                                self.converter
                                    .process_operation_update_message(discovered_ops),
                            )
                            .await?;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        eprintln!("Processing inotify event failed due to {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn process_sync_timeout(&mut self) -> Result<(), RuntimeError> {
        // Once the sync phase is complete, retrieve all sync messages from the converter and process them
        let sync_messages = self.converter.sync_messages();
        for message in sync_messages {
            self.process_mqtt_message(message).await?;
        }

        Ok(())
    }
}

/// Check if a message is an entity registration message.
fn is_entity_register_message(message: &Message) -> bool {
    let Ok(payload) = serde_json::from_slice::<serde_json::Value>(message.payload_bytes()) else {
        return false;
    };

    message.retain && payload.get("@type").is_some() && payload.get("type").is_some()
}

pub struct C8yMapperBuilder {
    config: C8yMapperConfig,
    box_builder: SimpleMessageBoxBuilder<C8yMapperInput, C8yMapperOutput>,
    mqtt_publisher: DynSender<MqttMessage>,
    http_proxy: C8YHttpProxy,
    timer_sender: DynSender<SyncStart>,
}

impl C8yMapperBuilder {
    pub fn try_new(
        config: C8yMapperConfig,
        mqtt: &mut impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter>,
        http: &mut impl ServiceProvider<C8YRestRequest, C8YRestResult, NoConfig>,
        timer: &mut impl ServiceProvider<SyncStart, SyncComplete, NoConfig>,
        fs_watcher: &mut impl MessageSource<FsWatchEvent, PathBuf>,
    ) -> Result<Self, FileError> {
        Self::init(&config)?;

        let box_builder = SimpleMessageBoxBuilder::new("CumulocityMapper", 16);

        let mqtt_publisher =
            mqtt.connect_consumer(config.topics.clone(), adapt(&box_builder.get_sender()));
        let http_proxy = C8YHttpProxy::new("C8yMapper => C8YHttpProxy", http);
        let timer_sender = timer.connect_consumer(NoConfig, adapt(&box_builder.get_sender()));
        fs_watcher.register_peer(config.ops_dir.clone(), adapt(&box_builder.get_sender()));

        Ok(Self {
            config,
            box_builder,
            mqtt_publisher,
            http_proxy,
            timer_sender,
        })
    }

    fn init(config: &C8yMapperConfig) -> Result<(), FileError> {
        // Create c8y operations directory
        create_directory_with_defaults(config.ops_dir.clone())?;
        // Create directory for device custom fragments
        create_directory_with_defaults(config.config_dir.join("device"))?;
        Ok(())
    }
}

impl RuntimeRequestSink for C8yMapperBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl Builder<C8yMapperActor> for C8yMapperBuilder {
    type Error = RuntimeError;

    fn try_build(self) -> Result<C8yMapperActor, Self::Error> {
        let mqtt_publisher = LoggingSender::new("C8yMapper => Mqtt".into(), self.mqtt_publisher);
        let timer_sender = LoggingSender::new("C8yMapper => Timer".into(), self.timer_sender);

        let device_id = self.config.device_id.clone();

        let converter =
            CumulocityConverter::new(self.config, mqtt_publisher.clone(), self.http_proxy)
                .map_err(|err| RuntimeError::ActorError(Box::new(err)))?;

        let message_box = self.box_builder.build();

        Ok(C8yMapperActor::new(
            converter,
            message_box,
            mqtt_publisher,
            timer_sender,
            device_id,
        ))
    }
}
