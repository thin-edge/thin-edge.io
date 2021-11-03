use crate::converter::*;
use crate::error::*;

use flockfile::{check_another_instance_is_not_running, Flockfile};

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

pub struct Mapper {
    client: mqtt_client::Client,
    config: MapperConfig,
    converter: Box<dyn Converter<Error = ConversionError>>,
    _flock: Flockfile,
}

impl Mapper {
    pub(crate) async fn run(&self) -> Result<(), MqttClientError> {
        info!("Running");
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

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_tests::test_mqtt_server::start_broker_local;
    use mqtt_tests::*;

    #[tokio::test]
    #[serial_test::serial]
    async fn a_valid_input_leads_to_a_translated_output() -> Result<(), anyhow::Error> {
        // Given an MQTT broker
        let mqtt_port: u16 = 55555;
        let _mqtt_server_handle = tokio::spawn(async move { start_broker_local(mqtt_port).await });

        // Given a mapper
        let name = "mapper_under_test";
        let mapper_config = MapperConfig {
            in_topic: Topic::new("in_topic")?,
            out_topic: Topic::new("out_topic")?,
            errors_topic: Topic::new("err_topic")?,
        };

        let flock = check_another_instance_is_not_running(name)
            .expect("Another mapper instance is locking /run/lock/mapper_under_test.lock");

        let mqtt_config = mqtt_client::Config::default().with_port(mqtt_port);
        let mqtt_client = Client::connect(name, &mqtt_config).await?;

        let mapper = Mapper {
            client: mqtt_client,
            config: mapper_config,
            converter: Box::new(UppercaseConverter),
            _flock: flock,
        };

        // Let's run the mapper in the background
        tokio::spawn(async move {
            let _ = mapper.run().await;
        });

        // One can now send requests
        // Happy path
        let input = "abcde";
        let expected = "ABCDE".to_string();
        let actual = received_on_published(mqtt_port, "in_topic", input, "out_topic", 5).await;
        assert_eq!(expected, actual?);

        // Ill-formed input
        let input = "éèê";
        let expected = format!("{}", UppercaseConverter::conversion_error());
        let actual = received_on_published(mqtt_port, "in_topic", input, "err_topic", 5).await;
        assert_eq!(expected, actual?);

        Ok(())
    }

    struct UppercaseConverter;

    impl UppercaseConverter {
        pub fn conversion_error() -> ConversionError {
            // Just a stupid error that matches the expectations of the mapper
            ConversionError::FromMapperError(MapperError::HomeDirNotFound)
        }
    }

    impl Converter for UppercaseConverter {
        type Error = ConversionError;

        fn convert(&self, input: &str) -> Result<String, Self::Error> {
            if input.is_ascii() {
                Ok(input.to_uppercase())
            } else {
                Err(UppercaseConverter::conversion_error())
            }
        }
    }
}
