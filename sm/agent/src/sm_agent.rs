use crate::component::TEdgeComponent;
use async_trait::async_trait;
use log::{debug, error, info};
use mqtt_client::{
    Client, Config, Message, MqttClient, MqttClientError, Payload, Topic, TopicFilter,
};
use std::path::PathBuf;
use std::sync::Arc;
use tedge_config::TEdgeConfig;
use tedge_sm_lib::{message::*, plugin::*, plugin_manager::*, software::*};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

#[derive(Debug)]
pub struct SmAgentConfig {
    pub in_topic: Topic,
    pub out_topic: Topic,
    pub errors_topic: Topic,
    pub mqtt_client_config: mqtt_client::Config,
}

impl Default for SmAgentConfig {
    fn default() -> Self {
        let in_topic = Topic::new("tedge/commands/software/req").expect("Invalid topic");
        let out_topic = Topic::new("tedge/commands/software/resp").expect("Invalid topic");
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

#[derive(Debug)]
pub struct SmAgent {
    config: SmAgentConfig,
    name: String,
}

impl Default for SmAgent {
    fn default() -> Self {
        Self::new("SmAgent")
    }
}

impl SmAgent {
    pub fn new(name: &str) -> Self {
        Self {
            config: SmAgentConfig::default(),
            name: name.into(),
        }
    }

    // #[async_trait]
    // impl TEdgeComponent for SmAgent {
    pub async fn start(&self) -> Result<(), anyhow::Error> {
        let name = "sm-agent";
        info!("Starting sm-agent");

        let request_topic = TopicFilter::new("tedge/commands/req/software/#")?;
        let response_topic = Topic::new("tedge/commands/res/software/list")?;
        let error_topic = Topic::new("tedge/errors")?;

        let plugins = Arc::new(ExternalPlugins::open("/etc/tedge/sm-plugins")?);
        if plugins.empty() {
            error!("Couldn't load plugins from /etc/tedge/sm-plugins");
            return Err(SmAgentError::NoPlugins.into());
        }

        let mqtt = Client::connect(self.name.as_str(), &self.config.mqtt_client_config).await?;
        let mut errors = mqtt.subscribe_errors();
        tokio::spawn(async move {
            while let Some(error) = errors.next().await {
                error!("{}", error);
            }
        });

        // * Maybe it would be nice if mapper/registry responds
        publish_capabilities(&mqtt).await?;

        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<Message>();
        // let listener = tokio::task::spawn(async move {
        let mut operations = mqtt.subscribe(request_topic).await?;
        while let Some(message) = operations.next().await {
            info!("Request {:?}", message);

            let operation: SoftwareOperation = message.topic.clone().into();
            dbg!(&operation);

            match operation {
                SoftwareOperation::CurrentSoftwareList => {
                    let plugins = plugins.clone();

                    let acquire_list = tokio::task::spawn_blocking(move || plugins.list());
                    let status = acquire_list.await??;

                    let request = match SoftwareRequestList::from_slice(message.payload_trimmed()) {
                        Ok(request) => request,
                        Err(error) => {
                            debug!("Parsing error: {}", error);
                            let _ = mqtt
                                .publish(Message::new(&error_topic, format!("{}", error)))
                                .await?;
                            continue;
                        }
                    };
                    dbg!(&request);

                    let response = SoftwareResponseList {
                        id: request.id,
                        status: SoftwareOperationResultStatus::Successful,
                        list: SoftwareOperationStatus::CurrentSoftwareList { list: status },
                    };

                    // * avoid alloc if possible
                    let payload = response.to_bytes()?;
                    let _ = mqtt.publish(Message::new(&response_topic, payload)).await?;
                }

                SoftwareOperation::SoftwareUpdates => {
                    let request = match SoftwareRequestUpdate::from_slice(message.payload_trimmed())
                    {
                        Ok(request) => {
                            let response = SoftwareResponseUpdateStatus {
                                id: request.id,
                                status: SoftwareOperationResultStatus::Executing,
                                current_software_list: None,
                                reason: None,
                            };
                            let _ = mqtt
                                .publish(Message::new(&response_topic, response.to_bytes()?))
                                .await?;
                            request
                        }
                        Err(error) => {
                            debug!("Parsing error: {}", error);
                            let _ = mqtt
                                .publish(Message::new(&error_topic, format!("{}", error)))
                                .await?;
                            continue;
                        }
                    };
                    dbg!(&request);

                    let plugins = plugins.clone();
                    let mut response = SoftwareResponseUpdateStatus {
                        id: request.id,
                        status: SoftwareOperationResultStatus::Failed,
                        current_software_list: None,
                        reason: None,
                    };

                    'main_list: for software_list_type in request.update_list {
                        dbg!();
                        // dbg!(&software_list_type);
                        let plugin = plugins
                            .by_software_type(&software_list_type.plugin_type)
                            .unwrap();

                        // TODO: Uncomment when plugin discovery done. Or add map to above option
                        // if plugins.is_none() {
                        //     return Err(SoftwareError::UnknownSoftwareType {
                        //         software_type: software_list_type.plugin_type,
                        //     }
                        //     .into());
                        // }

                        if let Err(e) = plugin.prepare() {
                            let _ = mqtt
                                .publish(Message::new(&response_topic, response.to_bytes()?))
                                .await?;
                        };

                        'sta: for module in software_list_type.modules {
                            dbg!();
                            dbg!(&module);
                            match module.action {
                                SoftwareRequestUpdateAction::Install => {
                                    if let Err(e) = plugin.install(&module) {
                                        response.reason = Some("Module installation failed".into());
                                        let _ = mqtt
                                            .publish(Message::new(
                                                &response_topic,
                                                response.to_bytes()?,
                                            ))
                                            .await?;
                                        break 'main_list;
                                    }
                                }

                                SoftwareRequestUpdateAction::Remove => {
                                    if let Err(e) = plugin.remove(&module) {
                                        response.reason = Some("Module installation failed".into());
                                        let _ = mqtt
                                            .publish(Message::new(
                                                &response_topic,
                                                response.to_bytes()?,
                                            ))
                                            .await?;
                                        break;
                                    }
                                }
                            }
                        }

                        let () = plugin.finalize()?;

                        // software_list_type.modules.into_iter().for_each(|module| {
                        //     plugins.apply(&module);
                        // });

                        // let blocking_task =
                        //     tokio::task::spawn_blocking(move || plugins.apply(&module));
                        // let status: SoftwareUpdateStatus = blocking_task.await?;
                    }

                    // for update in &updates {
                    //     let status = SoftwareUpdateStatus::scheduled(update);
                    //     let json = serde_json::to_string(&status)?;
                    //     let _ = mqtt.publish(Message::new(&response_topic, json)).await?;
                    // }

                    // for update in updates {
                    //     let plugins = plugins.clone();
                    //     let blocking_task =
                    //         tokio::task::spawn_blocking(move || plugins.apply(&update));
                    //     let status: SoftwareUpdateStatus = blocking_task.await?;
                    //     let json = serde_json::to_string(&status)?;
                    //     let _ = mqtt.publish(Message::new(&response_topic, json)).await?;
                    // }
                }

                // }
                _ => unimplemented!(),
            }
        }
        // });

        let dispatcher = tokio::task::spawn(async move {});

        let publisher = tokio::task::spawn(async move {});

        Ok(())
    }
}

async fn publish_capabilities(mqtt: &Client) -> Result<(), SmAgentError> {
    mqtt.publish(Message::new(&Topic::new("tedge/capabilities/software/list")?, "").retain())
        .await?;

    mqtt.publish(Message::new(&Topic::new("tedge/capabilities/software/update")?, "").retain())
        .await?;

    Ok(())
}

#[derive(thiserror::Error, Debug)]
pub enum SmAgentError {
    #[error("Couldn't load plugins from /etc/tedge/sm-plugins")]
    NoPlugins,

    #[error(transparent)]
    MqttClient(#[from] MqttClientError),

    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),

    #[error(transparent)]
    SoftwareError(#[from] SoftwareError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
