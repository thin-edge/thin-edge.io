use crate::error::*;
use crate::{converter::*, operations::Operations};

use c8y_smartrest::smartrest_serializer::{SmartRestSerializer, SmartRestSetSupportedOperations};
use flockfile::{check_another_instance_is_not_running, Flockfile};
use mqtt_client::{Client, MqttClient, MqttClientError, Topic};
use tedge_config::{ConfigSettingAccessor, MqttPortSetting, TEdgeConfig};
use tokio::task::JoinHandle;
use tracing::{error, info, instrument};

pub async fn create_mapper<'a>(
    app_name: &'a str,
    tedge_config: &'a TEdgeConfig,
    converter: Box<dyn Converter<Error = ConversionError>>,
) -> Result<Mapper, anyhow::Error> {
    let flock = check_another_instance_is_not_running(app_name)?;

    info!("{} starting", app_name);

    let mqtt_config = mqtt_config(tedge_config)?;
    let mqtt_client = Client::connect(app_name, &mqtt_config).await?;
    let operations = Operations::new("/etc/tedge/operations");
    dbg!(&operations);

    Ok(Mapper::new(mqtt_client, converter, operations, flock))
}

pub(crate) fn mqtt_config(
    tedge_config: &TEdgeConfig,
) -> Result<mqtt_client::Config, anyhow::Error> {
    Ok(mqtt_client::Config::default().with_port(tedge_config.query(MqttPortSetting)?.into()))
}

pub struct Mapper {
    client: mqtt_client::Client,
    converter: Box<dyn Converter<Error = ConversionError>>,
    operations: Operations,
    _flock: Flockfile,
}

impl Mapper {
    pub fn new(
        client: mqtt_client::Client,
        converter: Box<dyn Converter<Error = ConversionError>>,
        operations: Operations,
        _flock: Flockfile,
    ) -> Self {
        Self {
            client,
            converter,
            operations,
            _flock,
        }
    }

    pub(crate) async fn run(&mut self, cloud: &str) -> Result<(), MqttClientError> {
        info!("Running");
        let () = self.post_operations(cloud).await?;
        let errors_handle = self.subscribe_errors();
        let messages_handle = self.subscribe_messages();
        messages_handle.await?;
        errors_handle
            .await
            .map_err(|_| MqttClientError::JoinError)?;
        Ok(())
    }

    async fn post_operations(&self, cloud: &str) -> Result<(), MqttClientError> {
        let ops = self.operations.get_operations_list(cloud);

        if !ops.is_empty() {
            let ops_msg = SmartRestSetSupportedOperations::new(&ops);
            let topic = Topic::new_unchecked("c8y/s/us");

            self.client
                .publish(mqtt_client::Message::new(
                    &topic,
                    ops_msg.to_smartrest().unwrap(),
                ))
                .await?;
        }

        Ok(())
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
    async fn subscribe_messages(&mut self) -> Result<(), MqttClientError> {
        let mut messages = self
            .client
            .subscribe(self.converter.get_in_topic_filter().clone())
            .await?;

        while let Some(message) = messages.next().await {
            let converted_messages = self.converter.convert(&message);
            for converted_message in converted_messages.into_iter() {
                self.client.publish(converted_message).await?
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_client::{Message, Topic, TopicFilter};
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    #[serial_test::serial]
    async fn a_valid_input_leads_to_a_translated_output() -> Result<(), anyhow::Error> {
        // Given an MQTT broker
        let broker = mqtt_tests::test_mqtt_broker();

        // Given a mapper
        let name = "mapper_under_test";

        let flock = check_another_instance_is_not_running(name)
            .expect("Another mapper instance is locking /run/lock/mapper_under_test.lock");

        let mqtt_config = mqtt_client::Config::default().with_port(broker.port);
        let mqtt_client = Client::connect(name, &mqtt_config).await?;

        let mut mapper = Mapper {
            client: mqtt_client,
            converter: Box::new(UppercaseConverter::new()),
            _flock: flock,
            operations: todo!(),
        };

        // Let's run the mapper in the background
        tokio::spawn(async move {
            let _ = mapper.run("test").await;
        });
        sleep(Duration::from_secs(1)).await;

        // One can now send requests
        let timeout = Duration::from_secs(1);

        // Happy path
        let input = "abcde";
        let expected = Some("ABCDE".to_string());
        let actual = broker
            .wait_for_response_on_publish("in_topic", input, "out_topic", timeout)
            .await;
        assert_eq!(expected, actual);

        // Ill-formed input
        let input = "éèê";
        let expected = Some(format!("{}", UppercaseConverter::conversion_error()));
        let actual = broker
            .wait_for_response_on_publish("in_topic", input, "err_topic", timeout)
            .await;
        assert_eq!(expected, actual);

        Ok(())
    }

    struct UppercaseConverter {
        mapper_config: MapperConfig,
    }

    impl UppercaseConverter {
        pub fn new() -> UppercaseConverter {
            let mapper_config = MapperConfig {
                in_topic_filter: TopicFilter::new("in_topic").expect("invalid topic filter"),
                out_topic: Topic::new_unchecked("out_topic"),
                errors_topic: Topic::new_unchecked("err_topic"),
            };
            UppercaseConverter { mapper_config }
        }

        pub fn conversion_error() -> ConversionError {
            // Just a stupid error that matches the expectations of the mapper
            ConversionError::FromMapper(MapperError::HomeDirNotFound)
        }
    }

    impl Converter for UppercaseConverter {
        type Error = ConversionError;

        fn get_mapper_config(&self) -> &MapperConfig {
            &self.mapper_config
        }

        fn try_convert(&mut self, input: &Message) -> Result<Vec<Message>, Self::Error> {
            let input = input.payload_str().expect("utf8");
            if input.is_ascii() {
                let msg = vec![Message::new(
                    &self.mapper_config.out_topic,
                    input.to_uppercase(),
                )];
                Ok(msg)
            } else {
                Err(UppercaseConverter::conversion_error())
            }
        }
    }
}
