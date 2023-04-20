use super::config::C8yMapperConfig;
use super::config::MQTT_MESSAGE_SIZE_THRESHOLD;
use super::converter::CumulocityConverter;
use super::converter::CumulocityDeviceInfo;
use super::dynamic_discovery::process_inotify_events;
use super::mapper::CumulocityMapper;
use crate::core::converter::Converter;
use crate::core::converter::MapperConfig;
use crate::core::size_threshold::SizeThreshold;
use async_trait::async_trait;
use c8y_api::smartrest::operations::Operations;
use c8y_api::smartrest::topic::SMARTREST_PUBLISH_TOPIC;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use std::path::PathBuf;
use std::time::Duration;
use tedge_actors::adapt;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sender;
use tedge_actors::ServiceConsumer;
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;

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
    ) -> Self {
        Self {
            converter,
            messages,
            mqtt_publisher,
            timer_sender,
        }
    }

    async fn process_mqtt_message(&mut self, message: MqttMessage) -> Result<(), RuntimeError> {
        let converted_messages = self.converter.convert(&message).await;

        for converted_message in converted_messages.into_iter() {
            let _ = self.mqtt_publisher.send(converted_message).await;
        }

        Ok(())
    }

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

pub struct C8yMapperBuilder {
    config: C8yMapperConfig,
    box_builder: SimpleMessageBoxBuilder<C8yMapperInput, C8yMapperOutput>,
    mqtt_publisher: Option<DynSender<MqttMessage>>,
    http_proxy: Option<C8YHttpProxy>,
    timer_sender: Option<DynSender<SyncStart>>,
}

impl C8yMapperBuilder {
    //HIPPO: Accept all the providers as arguments
    pub fn new(config: C8yMapperConfig) -> Self {
        let box_builder = SimpleMessageBoxBuilder::new("CumulocityMapper", 16);

        Self {
            config,
            box_builder,
            mqtt_publisher: None,
            http_proxy: None,
            timer_sender: None,
        }
    }

    pub fn with_c8y_http_proxy(
        &mut self,
        http: &mut impl ServiceProvider<C8YRestRequest, C8YRestResult, NoConfig>,
    ) -> Result<(), LinkError> {
        self.http_proxy = Some(C8YHttpProxy::new("C8yMapper => C8Y", http));
        Ok(())
    }
}

impl ServiceConsumer<MqttMessage, MqttMessage, TopicFilter> for C8yMapperBuilder {
    fn get_config(&self) -> TopicFilter {
        let operations = Operations::try_new(format!(
            "{}/operations/c8y",
            self.config.config_dir.display()
        ))
        .unwrap(); //HIPPO change the get_config API or panic is fine as the failure is during the initialization?

        CumulocityMapper::subscriptions(&operations).unwrap() //HIPPO
    }

    fn set_request_sender(&mut self, sender: DynSender<MqttMessage>) {
        self.mqtt_publisher = Some(sender);
    }

    fn get_response_sender(&self) -> DynSender<MqttMessage> {
        adapt(&self.box_builder.get_sender())
    }
}

impl ServiceConsumer<SetTimeout<()>, Timeout<()>, NoConfig> for C8yMapperBuilder {
    fn get_config(&self) -> NoConfig {
        NoConfig
    }

    fn set_request_sender(&mut self, sender: DynSender<SetTimeout<()>>) {
        self.timer_sender = Some(sender);
    }

    fn get_response_sender(&self) -> DynSender<Timeout<()>> {
        adapt(&self.box_builder.get_sender())
    }
}

impl MessageSink<FsWatchEvent, PathBuf> for C8yMapperBuilder {
    fn get_config(&self) -> PathBuf {
        self.config.ops_dir.clone()
    }

    fn get_sender(&self) -> DynSender<FsWatchEvent> {
        tedge_actors::adapt(&self.box_builder.get_sender())
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
        let mqtt_publisher = self
            .mqtt_publisher
            .ok_or_else(|| LinkError::MissingPeer {
                role: "mqtt".into(),
            })
            .map(|mqtt_publisher| {
                LoggingSender::new("CumulocityMapper MQTT".into(), mqtt_publisher)
            })?;

        let http_proxy = self.http_proxy.ok_or_else(|| LinkError::MissingPeer {
            role: "http".to_string(),
        })?;

        let timer_sender = self
            .timer_sender
            .ok_or_else(|| LinkError::MissingPeer {
                role: "timer".to_string(),
            })
            .map(|timer_sender| {
                LoggingSender::new("CumulocityMapper Timer".into(), timer_sender)
            })?;

        let operations = Operations::try_new(self.config.ops_dir.clone())
            .map_err(|err| RuntimeError::ActorError(Box::new(err)))?;
        let child_ops = Operations::get_child_ops(self.config.ops_dir.clone())
            .map_err(|err| RuntimeError::ActorError(Box::new(err)))?;
        let size_threshold = SizeThreshold(MQTT_MESSAGE_SIZE_THRESHOLD);
        let device_info = CumulocityDeviceInfo {
            device_name: self.config.device_id.clone(),
            device_type: self.config.device_type.clone(),
            operations,
            service_type: self.config.service_type.clone(),
            c8y_host: self.config.c8y_host.clone(),
        };

        let mapper_config = MapperConfig {
            out_topic: Topic::new_unchecked("c8y/measurement/measurements/create"),
            errors_topic: Topic::new_unchecked("tedge/errors"),
        };

        let converter = CumulocityConverter::new(
            size_threshold,
            device_info,
            mqtt_publisher.clone(),
            http_proxy,
            &self.config.config_dir,
            self.config.logs_path.clone(),
            child_ops,
            mapper_config,
        )
        .map_err(|err| RuntimeError::ActorError(Box::new(err)))?;

        let message_box = self.box_builder.build();

        Ok(C8yMapperActor::new(
            converter,
            message_box,
            mqtt_publisher,
            timer_sender,
        ))
    }
}
