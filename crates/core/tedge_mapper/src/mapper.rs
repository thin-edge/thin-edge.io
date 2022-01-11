use crate::converter::*;
use crate::error::*;

use mqtt_channel::{
    Connection, Message, MqttError, SinkExt, StreamExt, UnboundedReceiver, UnboundedSender,
};
use tedge_config::{ConfigSettingAccessor, MqttPortSetting, TEdgeConfig};
use tracing::{error, info, instrument};

pub async fn create_mapper<'a>(
    app_name: &'a str,
    tedge_config: &'a TEdgeConfig,
    converter: Box<dyn Converter<Error = ConversionError>>,
) -> Result<Mapper, anyhow::Error> {
    info!("{} starting", app_name);

    let mapper_config = converter.get_mapper_config();
    let mqtt_client = Connection::connect(
        app_name,
        &mqtt_config(tedge_config)?,
        mapper_config.in_topic_filter.clone().into(),
    )
    .await?;

    Mapper::subscribe_errors(mqtt_client.errors);

    Ok(Mapper::new(
        mqtt_client.received,
        mqtt_client.published,
        converter,
    ))
}

pub(crate) fn mqtt_config(
    tedge_config: &TEdgeConfig,
) -> Result<mqtt_channel::Config, anyhow::Error> {
    Ok(mqtt_channel::Config::default()
        .with_port(tedge_config.query(MqttPortSetting)?.into())
        .with_max_packet_size(10 * 1024 * 1024))
}

pub struct Mapper {
    input: UnboundedReceiver<Message>,
    output: UnboundedSender<Message>,
    converter: Box<dyn Converter<Error = ConversionError>>,
}

impl Mapper {
    pub fn new(
        input: UnboundedReceiver<Message>,
        output: UnboundedSender<Message>,
        converter: Box<dyn Converter<Error = ConversionError>>,
    ) -> Self {
        Self {
            input,
            output,
            converter,
        }
    }

    pub(crate) async fn run(&mut self) -> Result<(), MqttError> {
        info!("Running");
        self.process_messages().await?;
        Ok(())
    }

    #[instrument(skip(errors), name = "errors")]
    fn subscribe_errors(mut errors: UnboundedReceiver<MqttError>) {
        tokio::spawn(async move {
            while let Some(error) = errors.next().await {
                error!("{}", error);
            }
        });
    }

    #[instrument(skip(self), name = "messages")]
    async fn process_messages(&mut self) -> Result<(), MqttError> {
        let init_messages = self.converter.init_messages();
        for init_message in init_messages.into_iter() {
            let _ = self.output.send(init_message).await;
        }

        while let Some(message) = &mut self.input.next().await {
            let converted_messages = self.converter.convert(&message);
            for converted_message in converted_messages.into_iter() {
                let _ = self.output.send(converted_message).await;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_channel::{Message, Topic, TopicFilter};
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    #[serial_test::serial]
    async fn a_valid_input_leads_to_a_translated_output() -> Result<(), anyhow::Error> {
        // Given an MQTT broker
        let broker = mqtt_tests::test_mqtt_broker();

        // Given a mapper
        let name = "mapper_under_test";

        let mqtt_config = mqtt_channel::Config::default().with_port(broker.port);
        let mqtt_client =
            Connection::connect(name, &mqtt_config, TopicFilter::new_unchecked("in_topic")).await?;

        let mut mapper = Mapper {
            input: mqtt_client.received,
            output: mqtt_client.published,
            converter: Box::new(UppercaseConverter::new()),
        };

        // Let's run the mapper in the background
        tokio::spawn(async move {
            let _ = mapper.run().await;
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
