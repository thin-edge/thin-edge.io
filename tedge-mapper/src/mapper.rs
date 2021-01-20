use c8y_json_translator::CumulocityJson;

use log;

use mqtt_client;

use tokio::task::JoinHandle;

pub const IN_TOPIC: &str = "tedge/measurements";
pub const C8Y_TOPIC_C8Y_JSON: &str = "c8y/measurement/measurements/create";
pub const ERRORS_TOPIC: &str = "tedge/errors";

pub struct Mapper {
    client: mqtt_client::Client,
    in_topic: mqtt_client::Topic,
    out_topic: mqtt_client::Topic,
    err_topic: mqtt_client::Topic,
}

impl Mapper {
    pub fn new_from_string(
        client: mqtt_client::Client,
        in_topic: &str,
        out_topic: &str,
        err_topic: &str,
    ) -> Result<Self, mqtt_client::Error> {
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

    fn subscribe_errors(&self) -> JoinHandle<()> {
        let mut errors = self.client.subscribe_errors();
        tokio::spawn(async move {
            while let Some(error) = errors.next().await {
                log::error!("{}", error);
            }
        })
    }

    async fn subscribe_messages(&mut self) -> Result<(), mqtt_client::Error> {
        let mut messages = self.client.subscribe(self.in_topic.filter()).await?;
        while let Some(message) = messages.next().await {
            log::debug!("Mapping {:?}", message);
            match Mapper::map(&message.payload) {
                Ok(mapped) => {
                    self.client
                        .publish(mqtt_client::Message::new(&self.out_topic, mapped))
                        .await?
                }
                Err(error) => {
                    log::debug!("Mapping error: {}", error);
                    self.client
                        .publish(mqtt_client::Message::new(
                            &self.err_topic,
                            error.to_string(),
                        ))
                        .await?
                }
            }
        }
        Ok(())
    }
    pub async fn run(mut self) -> Result<(), mqtt_client::Error> {
        let errors_handle = self.subscribe_errors();
        let messages_handle = self.subscribe_messages();
        messages_handle.await?;
        errors_handle
            .await
            .map_err(|_| mqtt_client::Error::JoinError)?;
        Ok(())
    }

    fn map(input: &[u8]) -> Result<Vec<u8>, c8y_json_translator::ThinEdgeJsonError> {
        CumulocityJson::from_thin_edge_json(input)
    }
}
