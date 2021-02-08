use c8y_translator_lib::CumulocityJson;

use log;

use client;

use tokio::task::JoinHandle;

pub const IN_TOPIC: &str = "tedge/measurements";
pub const C8Y_TOPIC_C8Y_JSON: &str = "c8y/measurement/measurements/create";
pub const ERRORS_TOPIC: &str = "tedge/errors";

pub struct Mapper {
    client: client::Client,
    in_topic: client::Topic,
    out_topic: client::Topic,
    err_topic: client::Topic,
}

impl Mapper {
    pub fn new_from_string(
        client: client::Client,
        in_topic: &str,
        out_topic: &str,
        err_topic: &str,
    ) -> Result<Self, client::Error> {
        Ok(Self::new(
            client,
            client::Topic::new(in_topic)?,
            client::Topic::new(out_topic)?,
            client::Topic::new(err_topic)?,
        ))
    }

    fn new(
        client: client::Client,
        in_topic: client::Topic,
        out_topic: client::Topic,
        err_topic: client::Topic,
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

    async fn subscribe_messages(&self) -> Result<(), client::Error> {
        let mut messages = self.client.subscribe(self.in_topic.filter()).await?;
        while let Some(message) = messages.next().await {
            log::debug!("Mapping {:?}", message);
            match Mapper::map(&message.payload) {
                Ok(mapped) => {
                    self.client
                        .publish(client::Message::new(&self.out_topic, mapped))
                        .await?;
                }
                Err(error) => {
                    log::debug!("Mapping error: {}", error);
                    self.client
                        .publish(client::Message::new(
                            &self.err_topic,
                            error.to_string(),
                        ))
                        .await?;
                }
            }
        }
        Ok(())
    }
    pub async fn run(self) -> Result<(), client::Error> {
        let errors_handle = self.subscribe_errors();
        let messages_handle = self.subscribe_messages();
        messages_handle.await?;
        errors_handle
            .await
            .map_err(|_| client::Error::JoinError)?;
        Ok(())
    }

    fn map(input: &[u8]) -> Result<Vec<u8>, c8y_translator_lib::ThinEdgeJsonError> {
        CumulocityJson::from_thin_edge_json(input)
    }
}
