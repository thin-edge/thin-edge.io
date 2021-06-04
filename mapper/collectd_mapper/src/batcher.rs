use mqtt_client::{Message, MqttClient, MqttMessageStream, Topic, TopicFilter};
use std::sync::Arc;
use thin_edge_json::{
    group::MeasurementGrouper, measurement::FlatMeasurementVisitor,
    serialize::ThinEdgeJsonSerializer,
};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    time::Duration,
};
use tracing::{error, log::warn};

use crate::collectd::CollectdMessage;
use crate::error::*;
use chrono::TimeZone;

#[derive(Debug)]
pub struct MessageBatch {
    min_timestamp: f64,
    max_timestamp: f64,
    message_grouper: MeasurementGrouper,
}

impl MessageBatch {
    fn start_batch(collectd_message: CollectdMessage) -> Result<Self, DeviceMonitorError> {
        let message_grouper = MeasurementGrouper::new();
        let timestamp = collectd_message.timestamp;
        let mut message_batch = Self {
            min_timestamp: timestamp,
            max_timestamp: timestamp,
            message_grouper,
        };

        message_batch.add_to_batch(collectd_message)?;

        Ok(message_batch)
    }

    fn accept(&self, collectd_message: &CollectdMessage, batching_window: Duration) -> bool {
        let delta = collectd_message.timestamp - self.min_timestamp;
        delta < batching_window.as_secs_f64()
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

        let timestamp = collectd_message.timestamp;
        if timestamp > self.max_timestamp {
            self.max_timestamp = timestamp;
        }

        Ok(())
    }

    fn end_batch(mut self) -> Result<MeasurementGrouper, DeviceMonitorError> {
        let millis = (1000.0 * self.min_timestamp).floor() as i64;
        let timestamp = chrono::Local
            .timestamp_millis_opt(millis)
            .single()
            .ok_or_else(|| DeviceMonitorError::InvalidUnixTimestamp { timestamp: self.min_timestamp })?;
        let timestamp_tz = timestamp.with_timezone(timestamp.offset());

        self.message_grouper.timestamp(&timestamp_tz)?;

        Ok(self.message_grouper)
    }
}

pub struct MessageBatcher {
    sender: UnboundedSender<MeasurementGrouper>,
    mqtt_client: Arc<dyn MqttClient>,
    source_topic_filter: TopicFilter,
    batching_window: Duration,
    batching_timeout: Duration,
}

impl MessageBatcher {
    pub fn new(
        sender: UnboundedSender<MeasurementGrouper>,
        mqtt_client: Arc<dyn MqttClient>,
        batching_window: Duration,
        source_topic_filter: TopicFilter,
    ) -> Self {
        let batching_timeout = batching_window / 5;
        Self {
            sender,
            mqtt_client,
            source_topic_filter,
            batching_window,
            batching_timeout,
        }
    }

    pub async fn run(&self) -> Result<(), DeviceMonitorError> {
        let mut messages = CollectdStream::new(
            self.mqtt_client
                .subscribe(self.source_topic_filter.clone())
                .await?,
        );

        loop {
            if let Some(message) = messages.take_received_message() {
                // Build a message batch until the batching window times out and return the batch
                let message_batch_result = self
                    .build_message_batch_with_timeout(message, &mut messages)
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
            } else {
                messages.expect_next().await;
            }
        }
    }

    async fn build_message_batch_with_timeout(
        &self,
        first_message: Message,
        messages: &mut CollectdStream,
    ) -> Result<MeasurementGrouper, DeviceMonitorError> {
        let collectd_message = CollectdMessage::parse_from(&first_message)?;
        let mut message_batch =
            MessageBatch::start_batch(collectd_message)?;

        loop {
            messages.expect_next_with_timeout(self.batching_timeout).await;

            let message = match messages.received_message() {
                Some(message) => message,
                None => break, // No message received within batching_timeout seconds
            };

            let collectd_message = match CollectdMessage::parse_from(message) {
                Ok(message) => message,
                Err(err) => {
                    error!("Error parsing collectd message: {}", err);
                    continue;   // Even if one message is faulty, we skip that one and keep building the batch
                },
            };

            if message_batch.accept(&collectd_message, self.batching_window) {
                message_batch.add_to_batch(collectd_message)?;
            } else {
                break; // The received message is not batched but kept in the messages stream
            }
        }

        Ok(message_batch.end_batch()?)
    }
}

struct CollectdStream {
    messages: Box<dyn MqttMessageStream>,
    already_received: Option<Message>,
}

impl CollectdStream {
    pub fn new(messages: Box<dyn MqttMessageStream>) -> CollectdStream {
        CollectdStream {
            messages,
            already_received: None,
        }
    }

    pub fn received_message(&self) -> Option<&Message> {
        self.already_received.as_ref()
    }

    pub fn take_received_message(&mut self) -> Option<Message> {
        self.already_received.take()
    }

    pub async fn expect_next(&mut self) {
        self.already_received = self.messages.next().await;
    }

    pub async fn expect_next_with_timeout(&mut self, timeout: Duration) {
        if let Err(_timeout) = tokio::time::timeout(timeout, self.expect_next()).await {
            self.already_received = None
        }
    }
}

pub struct MessageBatchPublisher {
    receiver: UnboundedReceiver<MeasurementGrouper>,
    mqtt_client: Arc<dyn MqttClient>,
    target_topic: Topic,
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
            target_topic,
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

        let tedge_message = Message::new(&self.target_topic, tedge_json_serializer.bytes()?);

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
    use tokio::time::{self, Instant};

    #[test]
    fn test_message_batch_processor() -> anyhow::Result<()> {
        let collectd_time = 1622820786.0;
        let batch_time = chrono::FixedOffset::east(1 * 3600)
            .ymd(2021, 6, 4)
            .and_hms(16, 33, 6);

        let collectd_message = CollectdMessage::new("temperature", "value", 32.5, collectd_time);
        let mut message_batch = MessageBatch::start_batch(collectd_message)?;

        let collectd_message = CollectdMessage::new("coordinate", "x", 50.0, collectd_time+0.01);
        message_batch.add_to_batch(collectd_message)?;

        let collectd_message = CollectdMessage::new("coordinate", "y", 70.0, collectd_time+0.02);
        message_batch.add_to_batch(collectd_message)?;

        let collectd_message = CollectdMessage::new("pressure", "value", 98.2, collectd_time+0.03);
        message_batch.add_to_batch(collectd_message)?;

        let collectd_message = CollectdMessage::new("coordinate", "z", 90.0, 1622820786.04);
        message_batch.add_to_batch(collectd_message)?;

        let message_grouper = message_batch.end_batch()?;

        assert_eq!(message_grouper.timestamp, Some(batch_time));

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
    async fn batch_publisher() -> anyhow::Result<()> {
        let mut message_grouper = MeasurementGrouper::new();
        message_grouper.measurement(Some("temperature"), "value", 32.5)?;

        let (_sender, receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mut mqtt_client = MockMqttClient::new();
        mqtt_client.expect_publish().times(1).returning(|message| {
            assert_eq!(message.topic.name, "tedge/measurements"); // The test assertion happens here
            Ok(123)
        });

        let mut publisher = MessageBatchPublisher::new(
            receiver,
            Arc::new(mqtt_client),
            Topic::new("tedge/measurements")?,
        );
        publisher.publish_as_mqtt_message(message_grouper).await?;

        Ok(())
    }

    #[tokio::test]
    async fn batching_with_window_timeout() -> anyhow::Result<()> {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mqtt_client = build_mock_mqtt_client();

        let mut seq = Sequence::new(); // To control the order of mock returns
        let mut message_stream = MockMqttMessageStream::default();
        message_stream
            .expect_next()
            .times(1)
            .in_sequence(&mut seq) // The first value to be returned by this mock stream
            .returning(|| {
                time::pause();
                let topic = Topic::new("collectd/localhost/temperature/value").unwrap();
                let message = Message::new(&topic, "123456789:32.5");
                Box::pin(ready(Some(message)))
            });

        message_stream
            .expect_next()
            .times(1)
            .in_sequence(&mut seq) // The second value to be returend by this mock stream
            .returning(|| {
                Box::pin(async {
                    time::advance(Duration::from_millis(100)).await; // Advance time, but stay within the batching window so that this message is part of the batch
                    let topic = Topic::new("collectd/localhost/pressure/value").unwrap();
                    let message = Message::new(&topic, "123456789:98.2");
                    Some(message)
                })
            });

        message_stream
            .expect_next()
            .times(1)
            .in_sequence(&mut seq) // The second value to be returend by this mock stream
            .returning(|| {
                Box::pin(async {
                    time::advance(Duration::from_millis(1000)).await; // Advance time beyond the batching window so that upcoming messages arrive after the window is closed
                    time::resume();
                    let topic = Topic::new("collectd/localhost/dummy/value").unwrap();
                    let message = Message::new(&topic, "123456789:98.2");
                    Some(message)
                })
            });

        // This third message from this stream will not even be read as the batching window has closed with the previous message
        message_stream
            .expect_next()
            .times(0)
            .in_sequence(&mut seq) // The third value to be returend by this mock stream
            .returning(|| {
                println!("Third message time: {:?}", Instant::now());
                Box::pin(async {
                    let topic = Topic::new("collectd/localhost/speed/value").unwrap();
                    let message = Message::new(&topic, "123456789:350");
                    Some(message)
                })
            });

        // Block the stream from the 4th invocation onwards
        message_stream
            .expect_next()
            .returning(|| Box::pin(pending())); // Block the stream with a pending future

        let builder = MessageBatcher::new(
            sender,
            Arc::new(mqtt_client),
            Duration::from_millis(500),
            TopicFilter::new("collectd/#")?.qos(QoS::AtMostOnce),
        );

        let mut collectd_stream = CollectdStream::new(Box::new(message_stream));
        collectd_stream.expect_next().await;
        let first_message = collectd_stream.take_received_message().unwrap();

        let message_grouper = builder
            .build_message_batch_with_timeout(first_message,  &mut collectd_stream)
            .await?;

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
            None // This measurement isn't included in the batch because it came after the batching window
        );

        Ok(())
    }

    #[tokio::test]
    async fn batching_with_invalid_messages_within_a_batch() -> anyhow::Result<()> {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mqtt_client = build_mock_mqtt_client();

        let message_stream = build_message_stream_from_messages(vec![
            ("collectd/localhost/temperature/value", 123456789.0, 32.5),
            ("collectd/pressure/value", 123456789.0, 98.0), // Erraneous collectd message with invalid topic
            ("collectd/localhost/speed/value", 123456789.0, 350.0),
        ]);

        let mut collectd_stream = CollectdStream::new(Box::new(message_stream));
        collectd_stream.expect_next().await;
        let first_message = collectd_stream.take_received_message().unwrap();

        let builder = MessageBatcher::new(
            sender,
            Arc::new(mqtt_client),
            Duration::from_millis(1000),
            TopicFilter::new("collectd/#")?.qos(QoS::AtMostOnce),
        );
        let message_grouper = builder
            .build_message_batch_with_timeout(first_message, &mut collectd_stream)
            .await?;

        assert_eq!(
            message_grouper.get_measurement_value(Some("temperature"), "value"),
            Some(32.5)
        );
        assert_eq!(
            message_grouper.get_measurement_value(Some("pressure"), "value"),
            None // This measurement isn't included in the batch because the value was erraneous
        );
        assert_eq!(
            message_grouper.get_measurement_value(Some("speed"), "value"),
            Some(350.0) // This measurement is included in the batch even though the last message was erraneous
        );

        Ok(())
    }

    #[tokio::test]
    async fn batching_only_time_related_measurements() -> anyhow::Result<()> {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mqtt_client = build_mock_mqtt_client();

        let message_stream = build_message_stream_from_messages(vec![
            ("collectd/localhost/temperature/value", 123456789.0, 32.5),
            ("collectd/localhost/pressure/value", 123456789.002, 98.0),
            ("collectd/localhost/speed/value", 123456799.0, 350.0),     // no the same second
        ]);

        let mut collectd_stream = CollectdStream::new(Box::new(message_stream));
        collectd_stream.expect_next().await;
        let first_message = collectd_stream.take_received_message().unwrap();

        let builder = MessageBatcher::new(
            sender,
            Arc::new(mqtt_client),
            Duration::from_millis(1000),
            TopicFilter::new("collectd/#")?.qos(QoS::AtMostOnce),
        );
        let message_grouper = builder
            .build_message_batch_with_timeout(first_message, &mut collectd_stream)
            .await?;

        assert_eq!(
            message_grouper.get_measurement_value(Some("temperature"), "value"),
            Some(32.5)  // included because this is the first message
        );
        assert_eq!(
            message_grouper.get_measurement_value(Some("pressure"), "value"),
            Some(98.0)  // included because in the same time window as the first
        );
        assert_eq!(
            message_grouper.get_measurement_value(Some("speed"), "value"),
            None // Excluded because emitted one second later
        );

        Ok(())
    }

    #[tokio::test]
    async fn batching_with_erraneous_first_message() -> anyhow::Result<()> {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mqtt_client = build_mock_mqtt_client();

        let message_stream = build_message_stream_from_messages(vec![]);
        let mut collectd_stream = CollectdStream::new(Box::new(message_stream));

        let topic = Topic::new("collectd/host/group/key")?;
        let invalid_collectd_message = Message::new(&topic, "123456789"); // Invalid payload

        let builder = MessageBatcher::new(
            sender,
            Arc::new(mqtt_client),
            Duration::from_millis(1000),
            TopicFilter::new("collectd/#")?.qos(QoS::AtMostOnce),
        );
        let result = builder
            .build_message_batch_with_timeout(
                invalid_collectd_message,
                &mut collectd_stream
            )
            .await;

        assert_matches!(
            result,
            Err(DeviceMonitorError::InvalidCollectdMeasurementError(_))
        );

        Ok(())
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
        message_map: Vec<(&'static str, f64, f64)>,
    ) -> MockMqttMessageStream {
        let mut seq = Sequence::new(); // To control the order of mock returns
        let mut message_stream = MockMqttMessageStream::default();

        for message in message_map {
            message_stream
                .expect_next()
                .times(1)
                .in_sequence(&mut seq) // The third value to be returend by this mock stream
                .returning(move || {
                    let topic = Topic::new(message.0).unwrap();
                    let message = Message::new(&topic, format!("{}:{}", message.1, message.2));
                    Box::pin(ready(Some(message)))
                });
        }

        // Block the stream from the 4th invocation onwards
        message_stream
            .expect_next()
            .returning(|| Box::pin(pending())); // Block the stream with a pending future

        return message_stream;
    }
}
