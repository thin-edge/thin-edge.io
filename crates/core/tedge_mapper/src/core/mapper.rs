use crate::c8y::dynamic_discovery::*;
use crate::core::converter::*;
use crate::core::error::*;
use c8y_api::smartrest::topic::SMARTREST_PUBLISH_TOPIC;
use mqtt_channel::Message;
use mqtt_channel::MqttError;
use mqtt_channel::SinkExt;
use mqtt_channel::StreamExt;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use mqtt_channel::UnboundedReceiver;
use mqtt_channel::UnboundedSender;
use std::path::Path;
use std::time::Duration;
use tedge_api::health::health_check_topics;
use tedge_api::health::health_status_down_message;
use tedge_api::health::health_status_up_message;
use tedge_api::health::send_health_status;
use tedge_utils::notify::fs_notify_stream;
use tedge_utils::notify::FsEvent;
use tracing::error;
use tracing::info;
use tracing::instrument;
use tracing::warn;
const SYNC_WINDOW: Duration = Duration::from_secs(3);
use std::result::Result::Ok;

pub fn mqtt_config(
    name: &str,
    host: &str,
    port: u16,
    topic_filter: TopicFilter,
) -> Result<mqtt_channel::Config, anyhow::Error> {
    let name_str = name.to_string();
    Ok(mqtt_channel::Config::default()
        .with_host(host)
        .with_port(port)
        .with_session_name(name)
        .with_subscriptions(topic_filter)
        .with_max_packet_size(10 * 1024 * 1024)
        .with_initial_message(move || health_status_up_message(&name_str))
        .with_last_will_message(health_status_down_message(name)))
}

pub struct Mapper {
    mapper_name: String,
    input: UnboundedReceiver<Message>,
    output: UnboundedSender<Message>,
    converter: Box<dyn Converter<Error = ConversionError>>,
    health_check_topics: TopicFilter,
}

impl Mapper {
    pub fn new(
        mapper_name: String,
        input: UnboundedReceiver<Message>,
        output: UnboundedSender<Message>,
        errors: UnboundedReceiver<MqttError>,
        converter: Box<dyn Converter<Error = ConversionError>>,
    ) -> Self {
        info!("{mapper_name} starting");
        let health_check_topics: TopicFilter = health_check_topics(&mapper_name);
        Self::subscribe_errors(errors);
        Self {
            mapper_name,
            input,
            output,
            converter,
            health_check_topics,
        }
    }

    pub(crate) async fn run(&mut self, ops_dir: Option<&Path>) -> Result<(), MapperError> {
        info!("Running");
        self.process_messages(ops_dir).await?;
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
    async fn process_messages(&mut self, ops_dir: Option<&Path>) -> Result<(), MapperError> {
        let init_messages = self.converter.init_messages();
        for init_message in init_messages.into_iter() {
            let _ = self.output.send(init_message).await;
        }

        // Start the sync phase here and process messages until the sync window times out
        let _ = tokio::time::timeout(SYNC_WINDOW, async {
            while let Some(message) = self.input.next().await {
                self.process_message(message).await;
            }
        })
        .await;

        // Once the sync phase is complete, retrieve all sync messages from the converter and process them
        let sync_messages = self.converter.sync_messages();
        for message in sync_messages {
            self.process_message(message).await;
        }

        process_messages(self, ops_dir).await?;
        Ok(())
    }

    async fn process_message(&mut self, message: Message) {
        if self.health_check_topics.accept(&message) {
            send_health_status(&mut self.output, &self.mapper_name).await;
        } else {
            let converted_messages = self.converter.convert(&message).await;

            for converted_message in converted_messages.into_iter() {
                let _ = self.output.send(converted_message).await;
            }
        }
    }
}

async fn process_messages(mapper: &mut Mapper, ops_dir: Option<&Path>) -> Result<(), MapperError> {
    if let Some(path) = ops_dir {
        let mut fs_notification_stream = fs_notify_stream(&[(
            path,
            None,
            &[
                FsEvent::DirectoryCreated,
                FsEvent::FileCreated,
                FsEvent::FileDeleted,
                FsEvent::Modified,
            ],
        )])?;

        // Send health status to confirm the mapper initialization is completed
        send_health_status(&mut mapper.output, &mapper.mapper_name).await;

        loop {
            tokio::select! {
                Some(message) =  mapper.input.next() => {
                    mapper.process_message(message).await;
                }
                Some((path, file_event)) = fs_notification_stream.rx.recv() => {

                    match file_event {
                        FsEvent::DirectoryCreated => {
                            if let Some(directory_name) =path.file_name() {
                                let child_id = directory_name.to_string_lossy().to_string();
                                let message = Message::new(
                                    &Topic::new_unchecked(SMARTREST_PUBLISH_TOPIC),
                                    format!("101,{child_id},{child_id},thin-edge.io-child"),
                                );
                                let _ = mapper.output.send(message).await;
                            }
                        },
                        _ => {
                            match  process_inotify_events(&path, file_event) {
                                Ok(Some(discovered_ops)) => {
                                     let _ = mapper.output.send(mapper.converter.process_operation_update_message(discovered_ops)).await;
                                }
                                Ok(None) => {}
                                Err(e) => {eprintln!("Processing inotify event failed due to {}", e);}
                            }
                        }
                    }

                }
            }
        }
    } else {
        while let Some(message) = mapper.input.next().await {
            mapper.process_message(message).await;
        }
        Ok(())
    }
}

#[cfg(test)]
pub async fn create_mapper(
    app_name: &str,
    mqtt_host: String,
    mqtt_port: u16,
    converter: Box<dyn Converter<Error = ConversionError>>,
) -> Result<Mapper, anyhow::Error> {
    use mqtt_channel::Connection;

    let health_check_topics: TopicFilter = health_check_topics(app_name);

    let mapper_config = converter.get_mapper_config();
    let mut topic_filter = mapper_config.in_topic_filter.clone();
    topic_filter.add_all(health_check_topics.clone());

    let mqtt_client =
        Connection::new(&mqtt_config(app_name, &mqtt_host, mqtt_port, topic_filter)?).await?;

    Ok(Mapper::new(
        app_name.to_string(),
        mqtt_client.received,
        mqtt_client.published,
        mqtt_client.errors,
        converter,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_json_diff::assert_json_include;
    use async_trait::async_trait;
    use mqtt_channel::Connection;
    use mqtt_channel::Message;
    use mqtt_channel::Topic;
    use mqtt_channel::TopicFilter;
    use serde_json::Value;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    #[serial_test::serial]
    async fn a_valid_input_leads_to_a_translated_output() -> Result<(), anyhow::Error> {
        // Given an MQTT broker
        let broker = mqtt_tests::test_mqtt_broker();

        // Given a mapper
        let name = "mapper_under_test";
        let mut mapper = create_mapper(
            name,
            "localhost".into(),
            broker.port,
            Box::new(UppercaseConverter::new()),
        )
        .await?;

        // Let's run the mapper in the background
        tokio::spawn(async move {
            let _ = mapper.run(None).await;
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

    #[cfg(test)]
    use serde_json::json;
    #[tokio::test]
    #[serial_test::serial]
    async fn health_check() -> Result<(), anyhow::Error> {
        // Given an MQTT broker
        let broker = mqtt_tests::test_mqtt_broker();

        // Given a mapper
        let name = "mapper_under_test";

        let mut mapper = create_mapper(
            name,
            "localhost".to_string(),
            broker.port,
            Box::new(UppercaseConverter::new()),
        )
        .await?;

        // Let's run the mapper in the background
        tokio::spawn(async move {
            let _ = mapper.run(None).await;
        });
        sleep(Duration::from_secs(1)).await;

        let health_check_topic = format!("tedge/health-check/{name}");
        let health_topic = format!("tedge/health/{name}");
        let health_status = broker
            .wait_for_response_on_publish(
                &health_check_topic,
                "",
                &health_topic,
                Duration::from_secs(1),
            )
            .await
            .expect("JSON status message");
        let health_status: Value = serde_json::from_str(health_status.as_str())?;
        assert_json_include!(actual: &health_status, expected: json!({"status": "up"}));
        assert!(health_status["pid"].is_number());

        let common_health_check_topic = "tedge/health-check";
        let health_status = broker
            .wait_for_response_on_publish(
                common_health_check_topic,
                "",
                &health_topic,
                Duration::from_secs(1),
            )
            .await
            .expect("JSON status message");
        let health_status: Value = serde_json::from_str(health_status.as_str())?;
        assert_json_include!(actual: &health_status, expected: json!({"status": "up"}));
        assert!(health_status["pid"].is_number());
        assert!(health_status["time"].is_number());

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

    #[async_trait]
    impl Converter for UppercaseConverter {
        type Error = ConversionError;

        fn get_mapper_config(&self) -> &MapperConfig {
            &self.mapper_config
        }

        async fn try_convert(&mut self, input: &Message) -> Result<Vec<Message>, Self::Error> {
            let input = input.payload_str().expect("utf8");
            if input.is_ascii() {
                let msg = vec![Message::new(
                    &self.mapper_config.out_topic,
                    input.to_uppercase(),
                )];
                anyhow::Result::Ok(msg)
            } else {
                Err(UppercaseConverter::conversion_error())
            }
        }
    }

    pub async fn create_mapper(
        app_name: &str,
        mqtt_host: String,
        mqtt_port: u16,
        converter: Box<dyn Converter<Error = ConversionError>>,
    ) -> Result<Mapper, anyhow::Error> {
        let health_check_topics: TopicFilter = health_check_topics(app_name);

        let mapper_config = converter.get_mapper_config();
        let mut topic_filter = mapper_config.in_topic_filter.clone();
        topic_filter.add_all(health_check_topics.clone());

        let mqtt_client =
            Connection::new(&mqtt_config(app_name, &mqtt_host, mqtt_port, topic_filter)?).await?;

        Ok(Mapper::new(
            app_name.to_string(),
            mqtt_client.received,
            mqtt_client.published,
            mqtt_client.errors,
            converter,
        ))
    }
}
