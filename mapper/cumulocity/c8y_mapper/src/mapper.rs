use crate::service::*;
use async_trait::async_trait;
use c8y_translator_lib::CumulocityJson;
use mqtt_client::Message;
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

    type Configuration = ();

    async fn create(_config: ()) -> Result<Self, Self::Error> {
        let config = mqtt_client::Config::default();
        let mqtt = mqtt_client::Client::connect(Self::NAME, &config).await?;

        Self::new_from_string(mqtt, IN_TOPIC, C8Y_TOPIC_C8Y_JSON, ERRORS_TOPIC)
    }

    async fn run(&mut self) -> Result<(), Self::Error> {
        self.run_mapper().await
    }

    async fn shutdown(self) -> Result<(), MapperError> {
        Ok(self.client.disconnect().await?)
    }
}

impl Mapper {
    fn new_from_string(
        client: mqtt_client::Client,
        in_topic: &str,
        out_topic: &str,
        err_topic: &str,
    ) -> Result<Self, MapperError> {
        Ok(Self::new(
            client,
            mqtt_client::Topic::new(in_topic)?,
            mqtt_client::Topic::new(out_topic)?,
            mqtt_client::Topic::new(err_topic)?,
        ))
    }

    fn new(
        client: mqtt_client::Client,
        in_topic: mqtt_client::Topic,
        out_topic: mqtt_client::Topic,
        err_topic: mqtt_client::Topic,
    ) -> Self {
        Self {
            client,
            in_topic,
            out_topic,
            err_topic,
        }
    }

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
