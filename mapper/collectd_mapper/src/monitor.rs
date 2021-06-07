use clock::WallClock;
use mqtt_client::{Client, MqttClient};
use std::sync::Arc;
use thin_edge_json::group::MeasurementGrouper;
use tracing::{instrument, log::error};

use crate::{
    batcher::{MessageBatchPublisher, MessageBatcher},
    error::DeviceMonitorError,
};

const DEFAULT_HOST: &str = "localhost";
const DEFAULT_PORT: u16 = 1883;
const DEFAULT_MQTT_CLIENT_ID: &str = "collectd-mapper";
const DEFAULT_BATCHING_WINDOW: u64 = 200;
const DEFAULT_MQTT_SOURCE_TOPIC: &str = "collectd/#";
const DEFAULT_MQTT_TARGET_TOPIC: &str = "tedge/measurements";

use mqtt_client::{QoS, Topic, TopicFilter};
use std::time::Duration;

#[derive(Debug)]
pub struct DeviceMonitorConfig {
    host: &'static str,
    port: u16,
    mqtt_client_id: &'static str,
    mqtt_source_topic: &'static str,
    mqtt_target_topic: &'static str,
    batching_window: u64,
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
        let config = mqtt_client::Config::new(
            self.device_monitor_config.host,
            self.device_monitor_config.port,
        )
        .queue_capacity(1024);
        let mqtt_client: Arc<dyn MqttClient> =
            Arc::new(Client::connect(self.device_monitor_config.mqtt_client_id, &config).await?);

        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let message_batch_producer = MessageBatcher::new(
            sender,
            mqtt_client.clone(),
            Duration::from_millis(self.device_monitor_config.batching_window),
            TopicFilter::new(self.device_monitor_config.mqtt_source_topic)?.qos(QoS::AtMostOnce),
            Arc::new(WallClock),
        );
        let join_handle1 = tokio::task::spawn(async move {
            match message_batch_producer.run().await {
                Ok(_) => error!("Unexpected end of message batcher thread"),
                Err(err) => error!("Error in message batcher thread: {}", err),
            }
        });

        let mut message_batch_consumer = MessageBatchPublisher::new(
            receiver,
            mqtt_client.clone(),
            Topic::new(self.device_monitor_config.mqtt_target_topic)?,
        );
        let join_handle2 = tokio::task::spawn(async move {
            message_batch_consumer.run().await;
        });

        let mut errors = mqtt_client.subscribe_errors();
        let join_handle3 = tokio::task::spawn(async move {
            while let Some(error) = errors.next().await {
                error!("MQTT error: {}", error);
            }
        });

        let _ = join_handle1.await;
        let _ = join_handle2.await;
        let _ = join_handle3.await;

        Ok(())
    }
}
