use mqtt_client::{Client, MqttClient};
use std::sync::Arc;
use thin_edge_json::group::MeasurementGrouper;
use tracing::{instrument, log::error};

use crate::batcher::{DeviceMonitorError, MessageBatchPublisher, MessageBatcher};

const DEFAULT_HOST: &str = "localhost";
const DEFAULT_PORT: u16 = 1883;
const CLIENT_ID: &str = "tedge-dm-agent";
const DEFAULT_STATS_COLLECTION_WINDOW: u64 = 1000;
const SOURCE_TOPIC: &str = "collectd/#";
const TARGET_TOPIC: &str = "tedge/measurements";

use mqtt_client::{QoS, Topic, TopicFilter};
use std::time::Duration;

#[derive(Debug)]
pub struct DeviceMonitor;

impl DeviceMonitor {
    #[instrument(name = "monitor")]
    pub async fn run() -> Result<(), DeviceMonitorError> {
        let config = mqtt_client::Config::new(DEFAULT_HOST, DEFAULT_PORT).queue_capacity(1024);
        let mqtt_client: Arc<dyn MqttClient> = Arc::new(Client::connect(CLIENT_ID, &config).await?);

        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let message_batch_producer = MessageBatcher::new(
            sender,
            mqtt_client.clone(),
            Duration::from_millis(DEFAULT_STATS_COLLECTION_WINDOW),
            TopicFilter::new(SOURCE_TOPIC)?.qos(QoS::AtMostOnce),
        );
        let join_handle1 = tokio::task::spawn(async move {
            match message_batch_producer.run().await {
                Ok(_) => error!("Unexpected end of message batcher thread"),
                Err(err) => error!("Error in message batcher thread: {}", err),
            }
        });

        let mut message_batch_consumer =
            MessageBatchPublisher::new(receiver, mqtt_client.clone(), Topic::new(TARGET_TOPIC)?);
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
