use crate::converter::*;
use crate::error::*;

use flockfile::{Flockfile, FlockfileError};
use mqtt_client::{Client, MqttClient, MqttClientError, Topic};
use tedge_config::{ConfigSettingAccessor, MqttPortSetting, TEdgeConfig};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, instrument};

#[derive(Debug)]
pub struct MapperConfig {
    pub in_topic: Topic,
    pub out_topic: Topic,
    pub errors_topic: Topic,
}

pub fn make_valid_topic_or_panic(topic_name: &str) -> Topic {
    Topic::new(topic_name).expect("This is a valid topic name")
}

pub async fn create_mapper<'a>(
    app_name: &'a str,
    tedge_config: &'a TEdgeConfig,
    mapper_config: impl Into<MapperConfig>,
    converter: Box<dyn Converter<Error = ConversionError>>,
) -> Result<Mapper, anyhow::Error> {
    let flock = check_another_instance_is_not_running(app_name)?;

    info!("{} starting", app_name);

    let mqtt_config = mqtt_config(tedge_config)?;
    let mqtt_client = Client::connect(app_name, &mqtt_config).await?;

    Ok(Mapper::new(mqtt_client, mapper_config, converter, flock))
}

pub(crate) fn mqtt_config(
    tedge_config: &TEdgeConfig,
) -> Result<mqtt_client::Config, anyhow::Error> {
    Ok(mqtt_client::Config::default().with_port(tedge_config.query(MqttPortSetting)?.into()))
}

fn check_another_instance_is_not_running(app_name: &str) -> Result<Flockfile, FlockfileError> {
    match flockfile::Flockfile::new_lock(format!("{}.lock", app_name)) {
        Ok(file) => Ok(file),
        Err(err) => {
            error!("Another instance of {} is running.", app_name);
            Err(err)
        }
    }
}

pub struct Mapper {
    client: mqtt_client::Client,
    config: MapperConfig,
    converter: Box<dyn Converter<Error = ConversionError>>,
    _flock: Flockfile,
}

impl Mapper {
    pub(crate) async fn run(&self) -> Result<(), MqttClientError> {
        let errors_handle = self.subscribe_errors();
        let messages_handle = self.subscribe_messages();
        messages_handle.await?;
        errors_handle
            .await
            .map_err(|_| MqttClientError::JoinError)?;
        Ok(())
    }

    pub fn new(
        client: mqtt_client::Client,
        config: impl Into<MapperConfig>,
        converter: Box<dyn Converter<Error = ConversionError>>,
        _flock: Flockfile,
    ) -> Self {
        Self {
            client,
            config: config.into(),
            converter,
            _flock,
        }
    }

    #[instrument(skip(self), name = "errors")]
    fn subscribe_errors(&self) -> JoinHandle<()> {
        let mut errors = self.client.subscribe_errors();
        tokio::spawn(async move {
            while let Some(error) = errors.next().await {
                error!("{}", error);
            }
        })
    }

    #[instrument(skip(self), name = "messages")]
    async fn subscribe_messages(&self) -> Result<(), MqttClientError> {
        let mut messages = self.client.subscribe(self.config.in_topic.filter()).await?;
        while let Some(message) = messages.next().await {
            debug!("Mapping {:?}", message.payload_str());
            match self.converter.convert(message.payload_str()?) {
                Ok(mapped) => {
                    self.client
                        .publish(mqtt_client::Message::new(&self.config.out_topic, mapped))
                        .await?;
                }
                Err(error) => {
                    debug!("Mapping error: {}", error);
                    self.client
                        .publish(mqtt_client::Message::new(
                            &self.config.errors_topic,
                            error.to_string(),
                        ))
                        .await?;
                }
            }
        }
        Ok(())
    }
}
