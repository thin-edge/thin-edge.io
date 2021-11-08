use crate::converter::*;
use crate::error::*;

use flockfile::{check_another_instance_is_not_running, Flockfile};

use mqtt_client::{Client, MqttClient, MqttClientError, Topic, TopicFilter};
use std::collections::HashSet;
use tedge_config::{ConfigSettingAccessor, MqttPortSetting, TEdgeConfig};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, instrument};

#[derive(Debug)]
pub struct MapperConfig {
    pub in_topic_filter: TopicFilter,
    pub out_topic: Topic,
    pub errors_topic: Topic,
}

pub fn make_valid_topic_or_panic(topic_name: &str) -> Topic {
    Topic::new(topic_name).expect("This is a valid topic name")
}

pub fn make_valid_topic_filter_or_panic(filter_name: &str) -> TopicFilter {
    TopicFilter::new(filter_name).expect("This is a valid topic filter name")
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
        let mut messages = self
            .client
            .subscribe(self.config.in_topic_filter.clone())
            .await?;

        let mut children: HashSet<String> = HashSet::new();

        while let Some(message) = messages.next().await {
            debug!("Mapping {:?}", message.payload_str());
            debug!("Topic {:?}", message.topic.name);

            // ? Validation for child device ID?
            match get_child_id_from_topic(message.clone().topic.name) {
                Some(child_id) if child_id.is_empty() => {
                    let error = MapperError::InvalidChildId { id: child_id };
                    error!("Child ID error: {}", error);
                    self.client
                        .publish(mqtt_client::Message::new(
                            &self.config.errors_topic,
                            error.to_string(),
                        ))
                        .await?;
                }
                Some(child_id) => {
                    info!("Child ID {:?}", child_id);
                    dbg!(&children);
                    if !children.contains(&child_id) {
                        children.insert(child_id.clone());
                        if let Some(message) = self
                            .converter
                            .convert_child_device_creation(child_id.as_str())
                        {
                            self.client.publish(message).await?;
                        };
                    }

                    match self
                        .converter
                        .convert_child_device_payload(message.payload_str()?, child_id.as_str())
                    {
                        Ok(mapped) => {
                            self.client
                                .publish(mqtt_client::Message::new(&self.config.out_topic, mapped))
                                .await?;
                        }
                        Err(error) => {
                            error!("Mapping error: {}", error);
                            self.client
                                .publish(mqtt_client::Message::new(
                                    &self.config.errors_topic,
                                    error.to_string(),
                                ))
                                .await?;
                        }
                    }
                }
                None => {
                    // parent device
                    match self.converter.convert(message.payload_str()?) {
                        Ok(mapped) => {
                            self.client
                                .publish(mqtt_client::Message::new(&self.config.out_topic, mapped))
                                .await?;
                        }
                        Err(error) => {
                            error!("Mapping error: {}", error);
                            self.client
                                .publish(mqtt_client::Message::new(
                                    &self.config.errors_topic,
                                    error.to_string(),
                                ))
                                .await?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn get_child_id_from_topic(topic: String) -> Option<String> {
    topic.strip_prefix("tedge/measurements/").map(String::from)
}

#[cfg(test)]
mod test {
    use super::*;
    use test_case::test_case;

    #[test_case("tedge/measurements/test", Some("test".to_string()); "valid child id")]
    #[test_case("tedge/measurements/", Some("".to_string()); "invalid child id (empty value)")]
    #[test_case("tedge/measurements", None; "invalid child id (parent topic)")]
    #[test_case("foo/bar", None; "invalid child id (invalid topic)")]
    fn extract_invalid_child_id(topic: &str, expected_child_id: Option<String>) {
        let in_topic = topic.to_string();
        let child_id = get_child_id_from_topic(in_topic);
        assert_eq!(child_id, expected_child_id)
    }
}
