use crate::collectd::{CollectdMessage, OwnedCollectdMessage};
use crate::error::*;
use crate::message_batcher;
use clock::Clock;
use mqtt_client::{Message, MqttClient, MqttMessageStream, Topic, TopicFilter};
use std::sync::Arc;
use thin_edge_json::{
    group::MeasurementGrouper, measurement::FlatMeasurementVisitor,
    serialize::ThinEdgeJsonSerializer,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{error, log::warn};

/// We start a new batch upon receiving a message whose timestamp is farther away to the
/// timestamp of first message in the batch than `delta` seconds.
///
/// Delta is inclusive.
struct CollectdTimestampDeltaCriterion {
    delta: f64,
}

impl message_batcher::BatchingCriterion<OwnedCollectdMessage> for CollectdTimestampDeltaCriterion {
    fn belongs_to_batch(
        &self,
        message: &OwnedCollectdMessage,
        message_batch: &message_batcher::MessageBatch<OwnedCollectdMessage>,
    ) -> bool {
        let delta = message_batch.first().timestamp() - message.timestamp();
        delta.abs() <= self.delta
    }
}

pub struct MessageBatcher {
    sender: UnboundedSender<MeasurementGrouper>,
    mqtt_client: Arc<dyn MqttClient>,
    source_topic_filter: TopicFilter,
    batcher: message_batcher::MessageBatcher<OwnedCollectdMessage>,
    clock: Arc<dyn Clock>,
}

impl MessageBatcher {
    pub fn new(
        sender: UnboundedSender<MeasurementGrouper>,
        mqtt_client: Arc<dyn MqttClient>,
        batching_window: chrono::Duration,
        source_topic_filter: TopicFilter,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let batcher = message_batcher::MessageBatcher::new(1000, batching_window)
            .with_batching_criterion(CollectdTimestampDeltaCriterion {
                delta: batching_window.num_seconds() as f64,
            });

        Self {
            sender,
            mqtt_client,
            source_topic_filter,
            batcher,
            clock,
        }
    }

    pub async fn run(&mut self) -> Result<(), DeviceMonitorError> {
        let mut messages = self
            .mqtt_client
            .subscribe(self.source_topic_filter.clone())
            .await?;

        let mut outputs = Vec::new();

        loop {
            let next_notify_in = self.process_outputs(&mut outputs);
            self.process_io(messages.as_mut(), next_notify_in, &mut outputs)
                .await;
        }
    }

    fn process_outputs(
        &self,
        outputs: &mut Vec<message_batcher::Output<OwnedCollectdMessage>>,
    ) -> std::time::Duration {
        // sentinel value to avoid having to deal with optional timeouts.
        let mut next_notification_at = self.clock.now() + chrono::Duration::hours(24);

        // Handle outputs
        for output in outputs.drain(..) {
            match output {
                message_batcher::Output::MessageBatch(batch) => {
                    // Send the current batch to the batch processor
                    let _ = group_messages(batch)
                        .map_err(|err| error!("Error while grouping the message batch: {}", err))
                        .and_then(|group| {
                            self.sender.send(group).map_err(|err| {
                                error!("Error while publishing a message batch: {}", err)
                            })
                        });
                }
                message_batcher::Output::NotifyAt(at) => {
                    next_notification_at = std::cmp::min(next_notification_at, at);
                }
            }
        }

        // XXX: Ugly conversion due to std::time, chrono and tokio::time.
        let next_notification_in = (next_notification_at - self.clock.now()).to_std().unwrap();

        next_notification_in
    }

    async fn process_io(
        &mut self,
        messages: &mut dyn MqttMessageStream,
        next_notify_in: std::time::Duration,
        outputs: &mut Vec<message_batcher::Output<OwnedCollectdMessage>>,
    ) {
        match tokio::time::timeout_at(
            tokio::time::Instant::now() + next_notify_in,
            messages.next(),
        )
        .await
        {
            Err(_) => {
                // Timeout fired. Inform functional core
                let now = self.clock.now();
                self.batcher
                    .handle(message_batcher::Input::Notify { now }, outputs);
            }
            Ok(Some(mqtt_message)) => {
                // got a message
                let received_at = self.clock.now();
                match CollectdMessage::parse_from(&mqtt_message) {
                    Ok(collectd_message) => self.batcher.handle(
                        message_batcher::Input::Message {
                            received_at,
                            message: collectd_message.into(),
                        },
                        outputs,
                    ),
                    Err(err) => {
                        error!("Error parsing collectd message: {}", err);
                    }
                }
            }
            Ok(None) => {
                // XXX: Not sure if this works!
                error!("MQTT connection closed. Retrying...");
            }
        }
    }
}

fn group_messages(
    message_batch: message_batcher::MessageBatch<OwnedCollectdMessage>,
) -> Result<MeasurementGrouper, DeviceMonitorError> {
    let mut message_grouper = MeasurementGrouper::new();
    message_grouper.timestamp(&message_batch.opened_at())?;

    for collectd_message in message_batch.iter_messages() {
        message_grouper.measurement(
            Some(collectd_message.metric_group_key()),
            collectd_message.metric_key(),
            collectd_message.metric_value(),
        )?;
    }
    Ok(message_grouper)
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
    use clock::WallClock;
    use futures::future::{pending, ready};
    use mockall::Sequence;
    use mqtt_client::MockMqttClient;
    use mqtt_client::MockMqttErrorStream;
    use mqtt_client::MockMqttMessageStream;
    use mqtt_client::QoS;
    use tokio::time::{self, Instant};

    #[test]
    #[cfg(todo)]
    fn test_message_batch_processor() -> anyhow::Result<()> {
        let collectd_message = CollectdMessage::new("temperature", "value", 32.5, 1.0);
        let mut message_batch = MessageBatch::start_batch(collectd_message, WallClock.now())?;

        let collectd_message = CollectdMessage::new("coordinate", "x", 50.0, 1.0);
        message_batch.add_to_batch(collectd_message)?;

        let collectd_message = CollectdMessage::new("coordinate", "y", 70.0, 1.0);
        message_batch.add_to_batch(collectd_message)?;

        let collectd_message = CollectdMessage::new("pressure", "value", 98.2, 1.0);
        message_batch.add_to_batch(collectd_message)?;

        let collectd_message = CollectdMessage::new("coordinate", "z", 90.0, 1.0);
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
    #[cfg(todo)]
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

        let clock = Arc::new(WallClock);
        let builder = MessageBatcher::new(
            sender,
            Arc::new(mqtt_client),
            Duration::from_millis(500),
            TopicFilter::new("collectd/#")?.qos(QoS::AtMostOnce),
            clock.clone(),
        );

        let first_message = message_stream.next().await.unwrap();

        let message_grouper = builder
            .build_message_batch_with_timeout(first_message, clock.now(), &mut message_stream)
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
    #[cfg(todo)]
    async fn batching_with_invalid_messages_within_a_batch() -> anyhow::Result<()> {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mqtt_client = build_mock_mqtt_client();

        let mut message_stream = build_message_stream_from_messages(vec![
            ("collectd/localhost/temperature/value", 32.5),
            ("collectd/pressure/value", 98.0), // Erraneous collectd message with invalid topic
            ("collectd/localhost/speed/value", 350.0),
        ]);

        let first_message = message_stream.next().await.unwrap();
        let clock = WallClock;
        let builder = MessageBatcher::new(
            sender,
            Arc::new(mqtt_client),
            Duration::from_millis(1000),
            TopicFilter::new("collectd/#")?.qos(QoS::AtMostOnce),
            Arc::new(clock.clone()),
        );
        let message_grouper = builder
            .build_message_batch_with_timeout(first_message, clock.now(), &mut message_stream)
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
    #[cfg(todo)]
    async fn batching_with_erraneous_first_message() -> anyhow::Result<()> {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mqtt_client = build_mock_mqtt_client();

        let mut message_stream = build_message_stream_from_messages(vec![]);

        let topic = Topic::new("collectd/host/group/key")?;
        let invalid_collectd_message = Message::new(&topic, "123456789"); // Invalid payload

        let clock = Arc::new(WallClock);
        let builder = MessageBatcher::new(
            sender,
            Arc::new(mqtt_client),
            Duration::from_millis(1000),
            TopicFilter::new("collectd/#")?.qos(QoS::AtMostOnce),
            clock.clone(),
        );
        let result = builder
            .build_message_batch_with_timeout(
                invalid_collectd_message,
                clock.now(),
                &mut message_stream,
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
        message_map: Vec<(&'static str, f64)>,
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
                    let message = Message::new(&topic, format!("123456789:{}", message.1));
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
