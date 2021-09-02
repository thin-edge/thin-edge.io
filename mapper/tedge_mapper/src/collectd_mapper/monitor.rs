use mqtt_client::{Client, Message, MqttClient};
use std::sync::Arc;
use tracing::{instrument, log::error};

use crate::collectd_mapper::batcher::MessageBatch;
use crate::collectd_mapper::collectd::CollectdMessage;
use crate::collectd_mapper::error::DeviceMonitorError;
use batcher::{BatchConfigBuilder, BatchDriver, BatchDriverInput, BatchDriverOutput, Batcher};
use mqtt_client::{QoS, Topic, TopicFilter};

const DEFAULT_HOST: &str = "localhost";
const DEFAULT_PORT: u16 = 1883;
const DEFAULT_MQTT_CLIENT_ID: &str = "collectd-mapper";
const DEFAULT_BATCHING_WINDOW: u32 = 200;
const DEFAULT_MAXIMUM_MESSAGE_DELAY: u32 = 50;
const DEFAULT_MESSAGE_LEAP_LIMIT: u32 = 0;
const DEFAULT_MQTT_SOURCE_TOPIC: &str = "collectd/#";
const DEFAULT_MQTT_TARGET_TOPIC: &str = "tedge/measurements";

#[derive(Debug)]
pub struct DeviceMonitorConfig {
    host: &'static str,
    port: u16,
    mqtt_client_id: &'static str,
    mqtt_source_topic: &'static str,
    mqtt_target_topic: &'static str,
    batching_window: u32,
    maximum_message_delay: u32,
    message_leap_limit: u32,
}

impl Default for DeviceMonitorConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST,
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

    #[instrument(name = "monitor")]
    pub async fn run(&self) -> Result<(), DeviceMonitorError> {
        let mqtt_config = mqtt_client::Config::new(
            self.device_monitor_config.host,
            self.device_monitor_config.port,
        )
        .queue_capacity(1024);
        let mqtt_client: Arc<dyn MqttClient> = Arc::new(
            Client::connect(self.device_monitor_config.mqtt_client_id, &mqtt_config).await?,
        );

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

        let input_mqtt_client = mqtt_client.clone();
        let input_topic =
            TopicFilter::new(self.device_monitor_config.mqtt_source_topic)?.qos(QoS::AtMostOnce);
        let mut collectd_messages = input_mqtt_client.subscribe(input_topic).await?;
        let input_join_handle = tokio::task::spawn(async move {
            loop {
                match collectd_messages.next().await {
                    Some(message) => match CollectdMessage::parse_from(&message) {
                        Ok(collectd_message) => {
                            let batch_input = BatchDriverInput::Event(collectd_message);
                            if let Err(err) = msg_send.send(batch_input).await {
                                error!("Error while processing a collectd message: {}", err);
                            }
                        }
                        Err(err) => {
                            error!("Error while decoding a collectd message: {}", err);
                        }
                    },
                    None => {
                        //If the message batching loop returns, it means the MQTT connection has closed
                        error!("MQTT connection closed. Retrying...");
                    }
                }
            }
        });

        let output_mqtt_client = mqtt_client.clone();
        let output_topic = Topic::new(self.device_monitor_config.mqtt_target_topic)?;
        let output_join_handle = tokio::task::spawn(async move {
            while let Some(BatchDriverOutput::Batch(messages)) = batch_recv.recv().await {
                match MessageBatch::thin_edge_json_bytes(messages) {
                    Ok(payload) => {
                        let tedge_message = Message::new(&output_topic, payload);
                        if let Err(err) = output_mqtt_client.publish(tedge_message).await {
                            error!("Error while sending a thin-edge json message: {}", err);
                        }
                    }
                    Err(err) => {
                        error!("Error while encoding a thin-edge json message: {}", err);
                    }
                }
            }
        });

        let mut errors = mqtt_client.subscribe_errors();
        let error_join_handle = tokio::task::spawn(async move {
            while let Some(error) = errors.next().await {
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
