use super::batcher::MessageBatch;
use super::collectd::CollectdMessage;
use super::error::DeviceMonitorError;
use batcher::BatchConfigBuilder;
use batcher::BatchDriver;
use batcher::BatchDriverInput;
use batcher::BatchDriverOutput;
use batcher::Batcher;
use mqtt_channel::Connection;
use mqtt_channel::Message;
use mqtt_channel::QoS;
use mqtt_channel::SinkExt;
use mqtt_channel::StreamExt;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use tedge_api::health::health_check_topics;
use tedge_api::health::health_status_down_message;
use tedge_api::health::health_status_up_message;
use tedge_api::health::send_health_status;
use tracing::error;
use tracing::info;
use tracing::instrument;

const DEFAULT_HOST: &str = "localhost";
const DEFAULT_PORT: u16 = 1883;
const DEFAULT_MQTT_CLIENT_ID: &str = "collectd-mapper";
const DEFAULT_BATCHING_WINDOW: u32 = 500;
const DEFAULT_MAXIMUM_MESSAGE_DELAY: u32 = 400; // Heuristic delay that should work out well on an Rpi
const DEFAULT_MESSAGE_LEAP_LIMIT: u32 = 0;
const DEFAULT_MQTT_SOURCE_TOPIC: &str = "collectd/#";
const DEFAULT_MQTT_TARGET_TOPIC: &str = "tedge/measurements";
const COLLECTD_MAPPER_NAME: &str = "tedge-mapper-collectd";

#[derive(Debug)]
pub struct DeviceMonitorConfig {
    host: String,
    port: u16,
    mqtt_client_id: &'static str,
    pub mqtt_source_topic: &'static str,
    mqtt_target_topic: &'static str,
    batching_window: u32,
    maximum_message_delay: u32,
    message_leap_limit: u32,
}

impl Default for DeviceMonitorConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
            mqtt_client_id: DEFAULT_MQTT_CLIENT_ID,
            mqtt_source_topic: DEFAULT_MQTT_SOURCE_TOPIC,
            mqtt_target_topic: DEFAULT_MQTT_TARGET_TOPIC,
            batching_window: DEFAULT_BATCHING_WINDOW,
            maximum_message_delay: DEFAULT_MAXIMUM_MESSAGE_DELAY,
            message_leap_limit: DEFAULT_MESSAGE_LEAP_LIMIT,
        }
    }
}

impl DeviceMonitorConfig {
    pub fn with_port(self, port: u16) -> Self {
        Self { port, ..self }
    }

    pub fn with_host(self, host: String) -> Self {
        Self { host, ..self }
    }
}

#[derive(Debug)]
pub struct DeviceMonitor {
    device_monitor_config: DeviceMonitorConfig,
}

impl DeviceMonitor {
    pub fn new(device_monitor_config: DeviceMonitorConfig) -> Self {
        Self {
            device_monitor_config,
        }
    }

    #[instrument(skip(self), name = "monitor")]
    pub async fn run(&self) -> Result<(), DeviceMonitorError> {
        let health_check_topics: TopicFilter = health_check_topics(COLLECTD_MAPPER_NAME);

        let mut input_topic = TopicFilter::new(self.device_monitor_config.mqtt_source_topic)?
            .with_qos(QoS::AtMostOnce);
        input_topic.add_all(health_check_topics.clone());

        let mqtt_config = mqtt_channel::Config::new(
            self.device_monitor_config.host.to_string(),
            self.device_monitor_config.port,
        )
        .with_session_name(self.device_monitor_config.mqtt_client_id)
        .with_subscriptions(input_topic)
        .with_initial_message(|| health_status_up_message(COLLECTD_MAPPER_NAME))
        .with_last_will_message(health_status_down_message(COLLECTD_MAPPER_NAME));

        let mqtt_client = Connection::new(&mqtt_config).await?;

        let batch_config = BatchConfigBuilder::new()
            .event_jitter(self.device_monitor_config.batching_window)
            .delivery_jitter(self.device_monitor_config.maximum_message_delay)
            .message_leap_limit(self.device_monitor_config.message_leap_limit)
            .build();
        let (msg_send, msg_recv) = tokio::sync::mpsc::channel(100);
        let (batch_send, mut batch_recv) = tokio::sync::mpsc::channel(100);
        let driver = BatchDriver::new(Batcher::new(batch_config), msg_recv, batch_send);
        let driver_join_handle = tokio::task::spawn(async move {
            match driver.run().await {
                Ok(_) => error!("Unexpected end of message batcher thread"),
                Err(err) => error!("Error in message batcher thread: {}", err),
            }
        });

        let mut collectd_messages = mqtt_client.received;
        let mut output_messages = mqtt_client.published.clone();

        // Send health status to confirm the mapper initialization is completed
        send_health_status(&mut output_messages, COLLECTD_MAPPER_NAME).await;

        let input_join_handle = tokio::task::spawn(async move {
            while let Some(message) = collectd_messages.next().await {
                if health_check_topics.accept(&message) {
                    send_health_status(&mut output_messages, "tedge-mapper-collectd").await;
                } else {
                    match CollectdMessage::parse_from(&message) {
                        Ok(collectd_message) => {
                            for msg in collectd_message {
                                let batch_input = BatchDriverInput::Event(msg);
                                if let Err(err) = msg_send.send(batch_input).await {
                                    error!("Error while processing a collectd message: {}", err);
                                }
                            }
                        }
                        Err(err) => {
                            error!("Error while decoding a collectd message: {}", err);
                        }
                    }
                }
            }

            // The MQTT connection has been closed by the process itself.
            info!("Stop batching");
            let eof = BatchDriverInput::Flush;
            msg_send.send(eof).await
        });

        let output_topic = Topic::new(self.device_monitor_config.mqtt_target_topic)?;
        let mut output_messages = mqtt_client.published;
        let output_join_handle = tokio::task::spawn(async move {
            loop {
                match batch_recv.recv().await {
                    None | Some(BatchDriverOutput::Flush) => {
                        break;
                    }
                    Some(BatchDriverOutput::Batch(messages)) => {
                        match MessageBatch::thin_edge_json_bytes(messages) {
                            Ok(payload) => {
                                let tedge_message = Message::new(&output_topic, payload);
                                if let Err(err) = output_messages.send(tedge_message).await {
                                    error!("Error while sending a thin-edge json message: {}", err);
                                }
                            }
                            Err(err) => {
                                error!("Error while encoding a thin-edge json message: {}", err);
                            }
                        }
                    }
                }
            }
            // All the messages forwarded for batching have been processed.
            info!("Batching done");
        });

        let mut mqtt_errors = mqtt_client.errors;
        let error_join_handle = tokio::task::spawn(async move {
            while let Some(error) = mqtt_errors.next().await {
                error!("MQTT error: {}", error);
            }
        });

        let _ = driver_join_handle.await;
        let _ = input_join_handle.await;
        let _ = output_join_handle.await;
        let _ = error_join_handle.await;

        Ok(())
    }
}
