use async_trait::async_trait;
use c8y_translator_lib::CumulocityJson;
use mqtt_client::Message;
use service::Service;
use thiserror::Error;
use tokio::select;

pub const IN_TOPIC: &str = "tedge/measurements";
pub const C8Y_TOPIC_C8Y_JSON: &str = "c8y/measurement/measurements/create";
pub const ERRORS_TOPIC: &str = "tedge/errors";

#[derive(Error, Debug)]
pub enum MapperError {
    #[error("Mqtt client error: {0}")]
    MqttClient(#[from] mqtt_client::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("MQTT error stream exhausted")]
    MqttErrorStreamExhausted,

    #[error("MQTT message stream exhausted")]
    MqttMessageStreamExhausted,
}

/// Configuration for the `Mapper` service.
pub struct MapperConfig {
    pub mqtt_config: mqtt_client::Config,
    pub in_topic: &'static str,
    pub out_topic: &'static str,
    pub err_topic: &'static str,
}

impl Default for MapperConfig {
    fn default() -> Self {
        Self {
            mqtt_config: mqtt_client::Config::default(),
            in_topic: IN_TOPIC,
            out_topic: C8Y_TOPIC_C8Y_JSON,
            err_topic: ERRORS_TOPIC,
        }
    }
}

pub struct Mapper {
    client: mqtt_client::Client,
    in_topic: mqtt_client::Topic,
    out_topic: mqtt_client::Topic,
    err_topic: mqtt_client::Topic,
}

#[async_trait]
impl Service for Mapper {
    const NAME: &'static str = "tedge-mapper";

    type Error = MapperError;

    type Configuration = MapperConfig;

    async fn setup(config: Self::Configuration) -> Result<Self, Self::Error> {
        let in_topic = mqtt_client::Topic::new(config.in_topic)?;
        let out_topic = mqtt_client::Topic::new(config.out_topic)?;
        let err_topic = mqtt_client::Topic::new(config.err_topic)?;
        let client = mqtt_client::Client::connect(Self::NAME, &config.mqtt_config).await?;

        Ok(Self {
            client,
            in_topic,
            out_topic,
            err_topic,
        })
    }

    async fn run(&mut self) -> Result<(), Self::Error> {
        self.run_mapper().await
    }

    async fn reload(self) -> Result<Self, Self::Error> {
        Ok(self)
    }

    async fn shutdown(self) -> Result<(), MapperError> {
        Ok(self.client.disconnect().await?)
    }
}

impl Mapper {
    fn map_message(&self, message: Message) -> Message {
        log::debug!("Mapping {:?}", message);

        Self::map(&message.payload)
            .map(|mapped| Message::new(&self.out_topic, mapped))
            .unwrap_or_else(|error| {
                log::debug!("Mapping error: {}", error);
                Message::new(&self.err_topic, error.to_string())
            })
    }

    fn map(input: &[u8]) -> Result<Vec<u8>, c8y_translator_lib::ThinEdgeJsonError> {
        CumulocityJson::from_thin_edge_json(input)
    }

    // NOTE: This is a method in `impl Mapper` and not in `impl Service for Mapper`, as the `async_trait`
    // macro complains about it.
    async fn run_mapper(&mut self) -> Result<(), MapperError> {
        let mut errors = self.client.subscribe_errors();
        let mut messages = self.client.subscribe(self.in_topic.filter()).await?;

        loop {
            select! {
               next_error = errors.next() => {
                    let error = next_error.ok_or(MapperError::MqttErrorStreamExhausted)?;
                    log::error!("{}", error);
                }

                next_message = messages.next() => {
                    let message = next_message.ok_or(MapperError::MqttMessageStreamExhausted)?;
                    let mapped_message = self.map_message(message);
                    self.client.publish(mapped_message).await?;
                }
            }
        }
    }
}
