use mqtt_client::{Message, MqttClient, MqttMessageStream, Topic, TopicFilter};
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
    time::sleep,
};
use tracing::{error, log::warn};

use crate::collectd::{self, CollectdMessage};

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
    pub fn start_batch(collectd_message: CollectdMessage) -> Result<Self, DeviceMonitorError> {
        let mut message_grouper = MeasurementGrouper::new();
        message_grouper.timestamp(&current_timestamp())?;

        let mut message_batch = Self { message_grouper };

        message_batch.add_to_batch(collectd_message)?;

        Ok(message_batch)
    }

    fn add_to_batch(
        &mut self,
        collectd_message: CollectdMessage,
    ) -> Result<(), DeviceMonitorError> {
        self.message_grouper.measurement(
            Some(collectd_message.metric_group_key),
            collectd_message.metric_key,
            collectd_message.metric_value,
        )?;

        Ok(())
    }

    pub fn end_batch(self) -> MeasurementGrouper {
        self.message_grouper
    }
}

pub struct MessageBatcher {
    sender: UnboundedSender<MeasurementGrouper>,
    mqtt_client: Arc<dyn MqttClient>,
    topic_filter: TopicFilter,
    batching_window: Duration,
}

impl MessageBatcher {
    pub fn new(
        sender: UnboundedSender<MeasurementGrouper>,
        mqtt_client: Arc<dyn MqttClient>,
        batching_window: Duration,
        source_topic_filter: TopicFilter,
    ) -> Self {
        Self {
            sender,
            mqtt_client,
            topic_filter: source_topic_filter,
            batching_window,
        }
    }

    pub async fn run(&self) -> Result<(), DeviceMonitorError> {
        let mut messages = self
            .mqtt_client
            .subscribe(self.topic_filter.clone())
            .await?;

        loop {
            match messages.next().await {
                Some(message) => {
                    // Build a message batch until the batching window times out and return the batch
                    let message_batch_result = self
                        .build_message_batch_with_timeout(message, messages.as_mut())
                        .await;

                    match message_batch_result {
                        Ok(message_batch) => {
                            // Send the current batch to the batch processor
                            let _ = self.sender.send(message_batch).map_err(|err| {
                                error!("Error while publishing a message batch: {}", err)
                            });
                        }
                        Err(err) => {
                            error!("Error while building a message batch: {}", err);
                        }
                    }
                }
                None => {
                    //If the message batching loop returns, it means the MQTT connection has closed
                    error!("MQTT connection closed. Retrying...");
                }
            }
        }
    }

    async fn build_message_batch_with_timeout(
        &self,
        first_message: Message,
        messages: &mut dyn MqttMessageStream,
    ) -> Result<MeasurementGrouper, DeviceMonitorError> {
        let collectd_message = CollectdMessage::parse_from(&first_message)?;
        let mut message_batch = MessageBatch::start_batch(collectd_message)?;

        loop {
            select! {
                maybe_message = messages.next() => {
                    match maybe_message {
                        Some(message) => {
                            let collectd_message = match CollectdMessage::parse_from(&message) {
                                Ok(message) => message,
                                Err(err) => {
                                    error!("Error parsing collectd message: {}", err);
                                    continue;   // Even if one message is faulty, we skip that one and keep building the batch
                                },
                            };
                            message_batch.add_to_batch(collectd_message)?;
                        }
                        None => break
                    }
                }

                _result = sleep(self.batching_window) => {
                    break;
                }
            }
        }

        Ok(message_batch.end_batch())
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
        target_topic: Topic,
    ) -> Self {
        Self {
            receiver,
            mqtt_client,
            topic: target_topic,
        }
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

    use super::*;
    use assert_matches::assert_matches;
    use futures::future::{pending, ready};
    use mockall::Sequence;
    use mqtt_client::MockMqttClient;
    use mqtt_client::MockMqttErrorStream;
    use mqtt_client::MockMqttMessageStream;
    use mqtt_client::QoS;
    use tokio::time::sleep;

    #[test]
    fn test_message_batch_processor() -> anyhow::Result<()> {
        let collectd_message = CollectdMessage::new("temperature", "value", 32.5);
        let mut message_batch = MessageBatch::start_batch(collectd_message)?;

        let collectd_message = CollectdMessage::new("coordinate", "x", 50.0);
        message_batch.add_to_batch(collectd_message)?;

        let collectd_message = CollectdMessage::new("coordinate", "y", 70.0);
        message_batch.add_to_batch(collectd_message)?;

        let collectd_message = CollectdMessage::new("pressure", "value", 98.2);
        message_batch.add_to_batch(collectd_message)?;

        let collectd_message = CollectdMessage::new("coordinate", "z", 90.0);
        message_batch.add_to_batch(collectd_message)?;

        let message_grouper = message_batch.end_batch();

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

    #[tokio::test]
    async fn batch_publisher() {
        let mut message_grouper = MeasurementGrouper::new();
        message_grouper
            .measurement(Some("temperature"), "value", 32.5)
            .unwrap();

        let (_sender, receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mut mqtt_client = MockMqttClient::new();
        mqtt_client.expect_publish().times(1).returning(|message| {
            assert_eq!(message.topic.name, "tedge/measurements"); //The test assertion happens here
            Ok(123)
        });

        let mut publisher = MessageBatchPublisher::new(
            receiver,
            Arc::new(mqtt_client),
            Topic::new("tedge/measurements").unwrap(),
        );
        publisher
            .publish_as_mqtt_message(message_grouper)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn batching_with_window_timeout() {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mqtt_client = build_mock_mqtt_client();

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
                    sleep(Duration::from_millis(1000)).await; //Sleep for a duration greater than the batching window
                    let topic = Topic::new("collectd/localhost/speed/value").unwrap();
                    let message = Message::new(&topic, "123456789:350");
                    Some(message)
                })
            });

        //Block the stream from the 4th invocation onwards
        message_stream
            .expect_next()
            .returning(|| Box::pin(pending())); //Block the stream with a pending future

        let first_message = message_stream.next().await.unwrap();
        let builder = MessageBatcher::new(
            sender,
            Arc::new(mqtt_client),
            Duration::from_millis(1000),
            TopicFilter::new("collectd/#").unwrap().qos(QoS::AtMostOnce),
        );
        let message_grouper = builder
            .build_message_batch_with_timeout(first_message, &mut message_stream)
            .await
            .unwrap();

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

    #[tokio::test]
    async fn batching_with_invalid_messages_within_a_batch() {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mqtt_client = build_mock_mqtt_client();

        let mut message_stream = build_message_stream_from_messages(vec![
            ("collectd/localhost/temperature/value", 32.5),
            ("collectd/pressure/value", 98.0), //Erraneous collectd message with invalid topic
            ("collectd/localhost/speed/value", 350.0),
        ]);

        let first_message = message_stream.next().await.unwrap();
        let builder = MessageBatcher::new(
            sender,
            Arc::new(mqtt_client),
            Duration::from_millis(1000),
            TopicFilter::new("collectd/#").unwrap().qos(QoS::AtMostOnce),
        );
        let message_grouper = builder
            .build_message_batch_with_timeout(first_message, &mut message_stream)
            .await
            .unwrap();

        assert_eq!(
            message_grouper.get_measurement_value(Some("temperature"), "value"),
            Some(32.5)
        );
        assert_eq!(
            message_grouper.get_measurement_value(Some("pressure"), "value"),
            None //This measurement isn't included in the batch because the value was erraneous
        );
        assert_eq!(
            message_grouper.get_measurement_value(Some("speed"), "value"),
            Some(350.0) //This measurement is included in the batch even though the last message was erraneous
        );
    }

    #[tokio::test]
    async fn batching_with_erraneous_first_message() {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mqtt_client = build_mock_mqtt_client();

        let mut message_stream = build_message_stream_from_messages(vec![]);

        let topic = Topic::new("collectd/host/group/key").unwrap();
        let invalid_collectd_message = Message::new(&topic, "123456789"); // Invalid payload

        let builder = MessageBatcher::new(
            sender,
            Arc::new(mqtt_client),
            Duration::from_millis(1000),
            TopicFilter::new("collectd/#").unwrap().qos(QoS::AtMostOnce),
        );
        let result = builder
            .build_message_batch_with_timeout(invalid_collectd_message, &mut message_stream)
            .await;

        assert_matches!(
            result,
            Err(DeviceMonitorError::InvalidCollectdMeasurementError(_))
        );
    }

    fn build_mock_mqtt_client() -> MockMqttClient {
        let mut mqtt_client = MockMqttClient::new();

        mqtt_client.expect_subscribe_errors().returning(|| {
            let error_stream = MockMqttErrorStream::default();
            Box::new(error_stream)
        });

        mqtt_client.expect_subscribe().returning(|_| {
            let message_stream = MockMqttMessageStream::default();
            Ok(Box::new(message_stream))
        });

        return mqtt_client;
    }

    fn build_message_stream_from_messages(
        message_map: Vec<(&'static str, f64)>,
    ) -> MockMqttMessageStream {
        let mut seq = Sequence::new(); //To control the order of mock returns
        let mut message_stream = MockMqttMessageStream::default();

        for message in message_map {
            message_stream
                .expect_next()
                .times(1)
                .in_sequence(&mut seq) //The third value to be returend by this mock stream
                .returning(move || {
                    let topic = Topic::new(message.0).unwrap();
                    let message = Message::new(&topic, format!("123456789:{}", message.1));
                    Box::pin(ready(Some(message)))
                });
        }

        //Block the stream from the 4th invocation onwards
        message_stream
            .expect_next()
            .returning(|| Box::pin(pending())); //Block the stream with a pending future

        return message_stream;
    }
}
