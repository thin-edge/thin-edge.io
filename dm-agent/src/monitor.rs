use mqtt_client::Client;
use std::sync::Arc;
use thin_edge_json::group::MeasurementGrouper;
use tracing::{instrument, log::error};

use crate::{
    batcher::{DeviceMonitorError, MessageBatchPublisher, MessageBatcher},
    mqtt::MqttClientImpl,
};

const DEFAULT_HOST: &str = "localhost";
const DEFAULT_PORT: u16 = 1883;
const CLIENT_ID: &str = "tedge-dm-agent";

#[derive(Debug)]
pub struct DeviceMonitor;

impl DeviceMonitor {
    #[instrument(name = "monitor")]
    pub async fn run() -> Result<(), DeviceMonitorError> {
        let config = mqtt_client::Config::new(DEFAULT_HOST, DEFAULT_PORT);
        let mqtt_client = Client::connect(CLIENT_ID, &config).await?;
        let mqtt_client = Arc::new(MqttClientImpl { mqtt_client });

        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let message_batch_producer = MessageBatcher::new(sender, mqtt_client.clone())?;
        let join_handle1 = tokio::task::spawn(async move {
            match message_batch_producer.run().await {
                Ok(_) => error!("Unexpected end of message batcher thread"),
                Err(err) => error!("Error in message batcher thread: {}", err),
            }
        });

        let mut message_batch_consumer = MessageBatchPublisher::new(receiver, mqtt_client.clone())?;
        let join_handle2 = tokio::task::spawn(async move {
            message_batch_consumer.run().await;
        });

        let _ = join_handle1.await;
        let _ = join_handle2.await;

        Ok(())
    }
}
