use crate::component::TEdgeComponent;
use async_trait::async_trait;
use log::{debug, error, info};
use mqtt_client::{Client, Config, Message, MqttClient, MqttClientError, Payload, Topic};
use std::path::PathBuf;
use std::sync::Arc;
use tedge_config::TEdgeConfig;
use tedge_software_management_lib::{message::*, plugin::*, plugin_manager::*, software::*};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

#[async_trait]
trait InternalModule {
    type Error;
    async fn run(&mut self) -> Result<(), Self::Error>;
}

struct RequestListener {
    sender: UnboundedSender<Message>,
    mqtt_client: Arc<dyn MqttClient>,
    topic: Topic,
}

impl RequestListener {
    fn new(
        sender: UnboundedSender<Message>,
        mqtt_client: Arc<dyn MqttClient>,
        topic: Topic,
    ) -> Self {
        Self {
            sender,
            mqtt_client,
            topic,
        }
    }
}

#[async_trait]
impl InternalModule for RequestListener {
    type Error = anyhow::Error;

    async fn run(&mut self) -> Result<(), Self::Error> {
        let mut messages = self.mqtt_client.subscribe(self.topic.filter()).await?;

        loop {
            match messages.next().await {
                Some(message) => self.sender.send(message)?,
                None => {
                    error!("Put some msg");
                    continue;
                }
            }
        }
    }
}

struct RequestExecutor {
    receiver: UnboundedReceiver<Message>,
    sender: UnboundedSender<Payload>,
}

#[async_trait]
impl InternalModule for RequestExecutor {
    type Error = anyhow::Error;

    async fn run(&mut self) -> Result<(), Self::Error> {
        while let Some(msg) = self.receiver.recv().await {
            // process
            // bla bla bla

            //send response
            let message = vec![];
            self.sender.send(message)?
        }

        Ok(())
    }
}

struct RequestResponder {
    mqtt_client: Arc<dyn MqttClient>,
    receiver: UnboundedReceiver<Payload>,
    topic: Topic,
}

#[async_trait]
impl InternalModule for RequestResponder {
    type Error = anyhow::Error;

    async fn run(&mut self) -> Result<(), Self::Error> {
        while let Some(msg) = self.receiver.recv().await {
            self.mqtt_client.publish(Message::new(&self.topic, msg));
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct SmManagerConfig {
    pub in_topic: Topic,
    pub out_topic: Topic,
    pub errors_topic: Topic,
    pub mqtt_client_config: mqtt_client::Config,
}

impl Default for SmManagerConfig {
    fn default() -> Self {
        let in_topic = Topic::new("tedge/ops/sm/req").expect("Invalid topic");
        let out_topic = Topic::new("tedge/ops/sm/resp").expect("Invalid topic");
        let errors_topic = Topic::new("tedge/errors").expect("Invalid topic");

        let mqtt_client_config = mqtt_client::Config::default().with_packet_size(50 * 1024);

        Self {
            in_topic,
            out_topic,
            errors_topic,
            mqtt_client_config,
        }
    }
}

pub struct SmManager {
    config: SmManagerConfig,
    name: String,
}

impl SmManager {
    pub fn new(name: &str) -> Self {
        Self {
            config: SmManagerConfig::default(),
            name: name.into(),
        }
    }

    // pub async fn run(&self) -> Result<(), MqttClientError> {
    //     let errors_handle = self.subscribe_errors();
    //     let messages_handle = self.subscribe_messages();
    //     messages_handle.await?;
    //     errors_handle
    //         .await
    //         .map_err(|_| MqttClientError::JoinError)?;
    //     Ok(())
    // }

    // fn subscribe_errors(&self) -> JoinHandle<()> {
    //     let mut errors = self.client.subscribe_errors();
    //     tokio::spawn(async move {
    //         while let Some(error) = errors.next().await {
    //             error!("{}", error);
    //         }
    //     })
    // }

    // async fn subscribe_messages(&self) -> Result<(), MqttClientError> {
    //     let mut messages = self.client.subscribe(self.config.in_topic.filter()).await?;
    //     while let Some(message) = messages.next().await {
    //         debug!("Mapping {:?}", message.payload_str());
    //         match self.converter.convert(message.payload_str()?) {
    //             Ok(mapped) => {
    //                 self.client
    //                     .publish(mqtt_client::Message::new(&self.config.out_topic, mapped))
    //                     .await?;
    //             }
    //             Err(error) => {
    //                 debug!("Mapping error: {}", error);
    //                 self.client
    //                     .publish(mqtt_client::Message::new(
    //                         &self.config.errors_topic,
    //                         error.to_string(),
    //                     ))
    //                     .await?;
    //             }
    //         }
    //     }
    //     Ok(())
    // }
}

impl Default for SmManager {
    fn default() -> Self {
        Self::new("SmManager")
    }
}

#[async_trait]
impl TEdgeComponent for SmManager {
    async fn start(&self) -> Result<(), anyhow::Error> {
        let name = "sm-agent";
        let request_topic = Topic::new("tedge/ops/sm/req")?;
        let response_topic = Topic::new("tedge/ops/sm/resp")?;
        let error_topic = Topic::new("tedge/errors")?;
        let plugins = Arc::new(ExternalPlugins::open("/etc/tedge/sm-plugins")?);

        info!("Starting sm-agent");

        let mqtt = Client::connect(self.name.as_str(), &self.config.mqtt_client_config).await?;
        let mut errors = mqtt.subscribe_errors();
        tokio::spawn(async move {
            while let Some(error) = errors.next().await {
                error!("{}", error);
            }
        });

        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<Message>();
        // let listener = tokio::task::spawn(async move {
        let mut operations = mqtt.subscribe(request_topic.filter()).await.unwrap();
        while let Some(message) = operations.next().await {
            debug!("Request {:?}", message);

            let payload = match String::from_utf8(message.payload_trimmed().into()) {
                Ok(utf8) => utf8,
                Err(error) => {
                    debug!("UTF8 error: {}", error);
                    let _ = mqtt
                        .publish(Message::new(&error_topic, format!("{}", error)))
                        .await?;
                    continue;
                }
            };

            let request = match SoftwareRequest::from_json(&payload) {
                Ok(request) => request,
                Err(error) => {
                    debug!("Parsing error: {}", error);
                    let _ = mqtt
                        .publish(Message::new(&error_topic, format!("{}", error)))
                        .await?;
                    continue;
                }
            };

            match request.operation {
                SoftwareOperation::CurrentSoftwareList { list: _ } => {
                    let plugins = plugins.clone();
                    let blocking_task = tokio::task::spawn_blocking(move || plugins.list());
                    let status = blocking_task.await??;
                    let json_msg = SoftwareListStore::new(status);
                    let json = serde_json::to_string(&json_msg)?;
                    // let json = serde_json::to_string(&status)?;
                    let _ = mqtt.publish(Message::new(&response_topic, json)).await?;
                }

                SoftwareOperation::SoftwareUpdates { updates } => {
                    for update in &updates {
                        let status = SoftwareUpdateStatus::scheduled(update);
                        let json = serde_json::to_string(&status)?;
                        let _ = mqtt.publish(Message::new(&response_topic, json)).await?;
                    }

                    for update in updates {
                        let plugins = plugins.clone();
                        let blocking_task =
                            tokio::task::spawn_blocking(move || plugins.apply(&update));
                        let status: SoftwareUpdateStatus = blocking_task.await?;
                        let json = serde_json::to_string(&status)?;
                        let _ = mqtt.publish(Message::new(&response_topic, json)).await?;
                    }
                }

                SoftwareOperation::DesiredSoftwareList { modules: _ } => {
                    unimplemented!();
                }
            }
        }
        // });

        let dispatcher = tokio::task::spawn(async move {});

        let publisher = tokio::task::spawn(async move {});

        Ok(())
    }
}
