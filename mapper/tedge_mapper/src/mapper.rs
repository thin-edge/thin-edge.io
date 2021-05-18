use crate::error::{ConversionError, MapperError};
use mqtt_client::Topic;
use tokio::task::JoinHandle;
use tracing::{debug, error, instrument};

#[derive(Debug)]
pub struct MapperConfig {
    pub in_topic: Topic,
    pub out_topic: Topic,
    pub errors_topic: Topic,
}

pub struct Mapper {
    client: mqtt_client::Client,
    config: MapperConfig,
    converter: Box<dyn Converter<Error = ConversionError>>,
}

pub trait Converter {
    type Error;

    fn convert(&self, input: &[u8]) -> Result<Vec<u8>, Self::Error>;
}

impl Mapper {
    pub(crate) async fn run(&self) -> Result<(), mqtt_client::Error> {
        let errors_handle = self.subscribe_errors();
        let messages_handle = self.subscribe_messages();
        messages_handle.await?;
        errors_handle
            .await
            .map_err(|_| mqtt_client::Error::JoinError)?;
        Ok(())
    }

    pub fn new(
        client: mqtt_client::Client,
        config: impl Into<MapperConfig>,
        converter: Box<dyn Converter<Error = ConversionError>>,
    ) -> Self {
        Self {
            client,
            config: config.into(),
            converter,
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
    async fn subscribe_messages(&self) -> Result<(), mqtt_client::Error> {
        let mut messages = self.client.subscribe(self.config.in_topic.filter()).await?;
        while let Some(message) = messages.next().await {
            debug!("Mapping {:?}", message);
            match self.converter.convert(&message.payload) {
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

// This should be used for c8y as well as Azure
pub(crate) fn is_smaller_than_size_threshold(
    input: &[u8],
    threshold: usize,
) -> Result<(), MapperError> {
    let size = std::mem::size_of_val(input);
    if size > threshold {
        Err(MapperError::MessageSizeError { threshold })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::mapper::is_smaller_than_size_threshold;

    #[test]
    fn test_is_smaller_than_size_threshold_positive() {
        let input = "test"; // 4 bytes
        let result = is_smaller_than_size_threshold(input.as_ref(), 4);
        assert!(result.is_ok())
    }

    #[test]
    fn test_is_smaller_than_size_threshold_negative() {
        let input = "test"; // 4 bytes
        let result = is_smaller_than_size_threshold(input.as_ref(), 3);
        assert!(result.is_err())
    }
}
