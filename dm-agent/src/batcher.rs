use crate::mqtt::{MqttClient, MqttMessageStream};
use async_timer::oneshot::{Oneshot, Timer};
use mqtt_client::{Message, QoS, Topic, TopicFilter};
use std::{sync::Arc, time::Duration};
use thin_edge_json::{
    group::MeasurementGrouper,
    measurement::current_timestamp,
    measurement::FlatMeasurementVisitor,
    serialize::{ThinEdgeJsonSerializationError, ThinEdgeJsonSerializer},
};
use tokio::{
    select,
    sync::mpsc::{error::SendError, UnboundedReceiver, UnboundedSender},
};
use tracing::{error, log::warn};

use crate::collectd::{self, CollectdMessage};

const DEFAULT_STATS_COLLECTION_WINDOW: u64 = 1000;

const SOURCE_TOPIC: &str = "collectd/#";
const TARGET_TOPIC: &str = "tedge/measurements";

#[derive(thiserror::Error, Debug)]
pub enum DeviceMonitorError {
    #[error(transparent)]
    MqttClientError(#[from] Arc<mqtt_client::Error>),

    #[error(transparent)]
    InvalidCollectdMeasurementError(#[from] collectd::CollectdError),

    #[error(transparent)]
    InvalidThinEdgeJsonError(#[from] thin_edge_json::group::MeasurementGrouperError),

    #[error(transparent)]
    ThinEdgeJsonSerializationError(#[from] ThinEdgeJsonSerializationError),

    #[error(transparent)]
    BatchingError(#[from] SendError<MeasurementGrouper>),
}

impl From<mqtt_client::Error> for DeviceMonitorError {
    fn from(error: mqtt_client::Error) -> Self {
        Self::MqttClientError(Arc::new(error))
    }
}

#[derive(Debug)]
pub struct MessageBatch {
    message_grouper: MeasurementGrouper,
}

impl MessageBatch {
    pub fn new() -> Self {
        Self {
            message_grouper: MeasurementGrouper::new(),
        }
    }

    pub fn start_batch(&mut self, message: Message) -> Result<(), DeviceMonitorError> {
        self.message_grouper.timestamp(&current_timestamp())?;

        self.add_to_batch(message)?;

        Ok(())
    }

    pub fn add_to_batch(&mut self, message: Message) -> Result<(), DeviceMonitorError> {
        let collectd_message = CollectdMessage::parse_from(&message)?;

        self.message_grouper.measurement(
            Some(collectd_message.metric_group_key),
            collectd_message.metric_key,
            collectd_message.metric_value,
        )?;

        Ok(())
    }

    pub fn end_batch(&mut self) -> Option<MeasurementGrouper> {
        if self.message_grouper.is_empty() {
            None
        } else {
            Some(std::mem::replace(
                &mut self.message_grouper,
                MeasurementGrouper::new(),
            ))
        }
    }
}

pub struct MessageBatcher {
    sender: UnboundedSender<MeasurementGrouper>,
    mqtt_client: Arc<dyn MqttClient>,
    topic_filter: TopicFilter,
}

impl MessageBatcher {
    pub fn new(
        sender: UnboundedSender<MeasurementGrouper>,
        mqtt_client: Arc<dyn MqttClient>,
    ) -> Result<Self, DeviceMonitorError> {
        let topic_filter = TopicFilter::new(SOURCE_TOPIC)?.qos(QoS::AtMostOnce);
        Ok(Self {
            sender,
            mqtt_client,
            topic_filter,
        })
    }

    pub async fn run(&self) -> Result<(), DeviceMonitorError> {
        let mut errors = self.mqtt_client.subscribe_errors();
        tokio::task::spawn(async move {
            while let Some(error) = errors.next().await {
                error!("MQTT error: {}", error);
            }
        });

        let mut messages = self
            .mqtt_client
            .subscribe(self.topic_filter.clone())
            .await?;

        loop {
            match self.run_message_batcher(messages.as_mut()).await {
                Ok(_) => {
                    //If the message batching loop returns, it means the MQTT connection has closed
                    error!("MQTT connection closed");
                    break;
                }
                Err(error) => {
                    error!("Error while building a message batch: {}", error);
                }
            }
        }
        Ok(())
    }

    async fn run_message_batcher(
        &self,
        messages: &mut dyn MqttMessageStream,
    ) -> Result<(), DeviceMonitorError> {
        let batching_window = Duration::from_millis(DEFAULT_STATS_COLLECTION_WINDOW);
        let mut message_batch = MessageBatch::new();
        while let Some(message) = messages.next().await {
            //Start a new message batch
            message_batch.start_batch(message)?;

            //Add messages to the current batch until the batching window times out and return
            self.build_message_batch_with_timeout(&mut message_batch, messages, batching_window)
                .await?;

            //Close the current batch and send it to the batch processor
            if let Some(batch) = message_batch.end_batch() {
                self.sender.send(batch)?;
            }
        }

        Ok(())
    }

    async fn build_message_batch_with_timeout(
        &self,
        message_batch: &mut MessageBatch,
        messages: &mut dyn MqttMessageStream,
        batching_window: Duration,
    ) -> Result<(), DeviceMonitorError> {
        // start a timer that will close the current batch after a timeout
        let mut timeout = Timer::new(batching_window);

        loop {
            select! {
                maybe_message = messages.next() => {
                    match maybe_message {
                        Some(message) => message_batch.add_to_batch(message)?,
                        None => break
                    }
                }

                _result = &mut timeout => {
                    break;
                }
            }
        }

        Ok(())
    }
}

pub struct MessageBatchPublisher {
    receiver: UnboundedReceiver<MeasurementGrouper>,
    mqtt_client: Arc<dyn MqttClient>,
    topic: Topic,
}

impl MessageBatchPublisher {
    pub fn new(
        receiver: UnboundedReceiver<MeasurementGrouper>,
        mqtt_client: Arc<dyn MqttClient>,
    ) -> Result<Self, DeviceMonitorError> {
        let topic = Topic::new(TARGET_TOPIC)?;

        Ok(Self {
            receiver,
            mqtt_client,
            topic,
        })
    }

    pub async fn run(&mut self) {
        while let Some(message_grouper) = self.receiver.recv().await {
            if let Err(err) = self.publish_as_mqtt_message(message_grouper).await {
                error!("Error publishing the measurement batch: {}", err);
            }
        }

        warn!("MQTT message channel closed. Can not proceed");
    }

    async fn publish_as_mqtt_message(
        &mut self,
        message_grouper: MeasurementGrouper,
    ) -> Result<(), DeviceMonitorError> {
        let mut tedge_json_serializer = ThinEdgeJsonSerializer::new();
        message_grouper.accept(&mut tedge_json_serializer)?;

        let tedge_message = Message::new(&self.topic, tedge_json_serializer.bytes()?);

        self.mqtt_client.publish(tedge_message).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use crate::mqtt::MockMqttClient;
    use crate::mqtt::MockMqttErrorStream;
    use crate::mqtt::MockMqttMessageStream;
    use assert_matches::assert_matches;
    use futures::future::{pending, ready};
    use mockall::Sequence;
    use tokio::time::sleep;

    use super::*;

    #[test]
    fn test_message_batch_processor() -> anyhow::Result<()> {
        let mut message_batch = MessageBatch::new();

        let topic = Topic::new("collectd/localhost/temperature/value").unwrap();
        let collectd_message = Message::new(&topic, "123456789:32.5");
        message_batch.start_batch(collectd_message)?;

        let topic = Topic::new("collectd/localhost/coordinate/x").unwrap();
        let collectd_message = Message::new(&topic, "123456789:50");
        message_batch.add_to_batch(collectd_message)?;

        let topic = Topic::new("collectd/localhost/coordinate/y").unwrap();
        let collectd_message = Message::new(&topic, "123456789:70");
        message_batch.add_to_batch(collectd_message)?;

        let topic = Topic::new("collectd/localhost/pressure/value").unwrap();
        let collectd_message = Message::new(&topic, "123456789:98.2");
        message_batch.add_to_batch(collectd_message)?;

        let topic = Topic::new("collectd/localhost/coordinate/z").unwrap();
        let collectd_message = Message::new(&topic, "123456789:90");
        message_batch.add_to_batch(collectd_message)?;

        let message_grouper = message_batch.end_batch().unwrap();

        assert_matches!(message_grouper.timestamp, Some(_));

        assert_eq!(
            message_grouper.get_measurement_value(Some("temperature"), "value"),
            Some(32.5)
        );
        assert_eq!(
            message_grouper.get_measurement_value(Some("pressure"), "value"),
            Some(98.2)
        );
        assert_eq!(
            message_grouper.get_measurement_value(Some("coordinate"), "x"),
            Some(50.0)
        );
        assert_eq!(
            message_grouper.get_measurement_value(Some("coordinate"), "y"),
            Some(70.0)
        );
        assert_eq!(
            message_grouper.get_measurement_value(Some("coordinate"), "z"),
            Some(90.0)
        );

        Ok(())
    }

    #[test]
    fn empty_batch() {
        let mut message_batch = MessageBatch::new();
        let message_grouper = message_batch.end_batch();

        assert_matches!(message_grouper, None);
    }

    #[test]
    fn invalid_collectd_message_format() {
        let mut message_batch = MessageBatch::new();

        let topic = Topic::new("collectd/host/group/key").unwrap();
        let invalid_collectd_message = Message::new(&topic, "123456789");
        let result = message_batch.start_batch(invalid_collectd_message);

        assert_matches!(
            result,
            Err(DeviceMonitorError::InvalidCollectdMeasurementError(_))
        );
    }

    #[tokio::test]
    async fn batch_publisher() {
        let mut message_grouper = MeasurementGrouper::new();
        message_grouper
            .measurement(Some("temperature"), "value", 32.5)
            .unwrap();

        let (_sender, receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mut mqtt_client = MockMqttClient::new();
        mqtt_client.expect_publish().times(1).returning(|message| {
            assert_eq!(message.topic.name, TARGET_TOPIC.to_string()); //The test assertion happens here
            Ok(123)
        });

        let mut publisher = MessageBatchPublisher::new(receiver, Arc::new(mqtt_client)).unwrap();
        publisher
            .publish_as_mqtt_message(message_grouper)
            .await
            .unwrap();
    }

    //TODO Control the timeout better with mocked clocks
    #[tokio::test]
    async fn batching_with_window_timeout() {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mut mqtt_client = MockMqttClient::new();

        mqtt_client.expect_subscribe_errors().returning(|| {
            let error_stream = MockMqttErrorStream::default();
            Box::new(error_stream)
        });

        mqtt_client.expect_subscribe().returning(|_| {
            let message_stream = MockMqttMessageStream::default();
            Ok(Box::new(message_stream))
        });

        let mut seq = Sequence::new(); //To control the order of mock returns
        let mut message_stream = MockMqttMessageStream::default();
        message_stream
            .expect_next()
            .times(1)
            .in_sequence(&mut seq) //The first value to be returned by this mock stream
            .returning(|| {
                let topic = Topic::new("collectd/localhost/temperature/value").unwrap();
                let message = Message::new(&topic, "123456789:32.5");
                Box::pin(ready(Some(message)))
            });

        message_stream
            .expect_next()
            .times(1)
            .in_sequence(&mut seq) //The second value to be returend by this mock stream
            .returning(|| {
                let topic = Topic::new("collectd/localhost/pressure/value").unwrap();
                let message = Message::new(&topic, "123456789:98.2");
                Box::pin(ready(Some(message)))
            });

        //The third message from this stream will be returned only after the batching window
        message_stream
            .expect_next()
            .times(1)
            .in_sequence(&mut seq) //The third value to be returend by this mock stream
            .returning(|| {
                Box::pin(async {
                    sleep(Duration::from_millis(10000)).await; //Sleep for a duration greater than the batching window
                    let topic = Topic::new("collectd/localhost/speed/value").unwrap();
                    let message = Message::new(&topic, "123456789:350");
                    Some(message)
                })
            });

        //Block the stream from the 4th invocation onwards
        message_stream
            .expect_next()
            .returning(|| Box::pin(pending())); //Block the stream with a pending future

        let mut message_batch = MessageBatch::new();

        let builder = MessageBatcher::new(sender, Arc::new(mqtt_client)).unwrap();
        builder
            .build_message_batch_with_timeout(
                &mut message_batch,
                &mut message_stream,
                Duration::from_millis(1000),
            )
            .await
            .unwrap();

        let message_grouper = message_batch.end_batch().unwrap();

        assert_eq!(
            message_grouper.get_measurement_value(Some("temperature"), "value"),
            Some(32.5)
        );
        assert_eq!(
            message_grouper.get_measurement_value(Some("pressure"), "value"),
            Some(98.2)
        );
        assert_eq!(
            message_grouper.get_measurement_value(Some("speed"), "value"),
            None //This measurement isn't included in the batch because it came after the batching window
        );
    }
}
