use crate::collectd::{CollectdMessage, OwnedCollectdMessage};
use crate::error::*;
use clock::Clock;
use message_algos::{
    Envelope, GroupingPolicy, MessageBatcher as Batcher, MessageGroup, MessageGrouper,
    RetirementDecision, RetirementPolicy, Timestamp,
};
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
struct CollectdTimestampDeltaGroupingPolicy {
    delta: f64,
}

impl GroupingPolicy for CollectdTimestampDeltaGroupingPolicy {
    type Message = OwnedCollectdMessage;

    fn belongs_to_group(
        &self,
        message: &Envelope<Self::Message>,
        group: &MessageGroup<Self::Message>,
    ) -> bool {
        let delta = group.first().message.timestamp() - message.message.timestamp();
        delta.abs() <= self.delta
    }
}

struct MaxGroupAgeRetirementPolicy {
    max_group_age: chrono::Duration,
}

impl RetirementPolicy for MaxGroupAgeRetirementPolicy {
    type Message = OwnedCollectdMessage;

    fn check_retirement(
        &self,
        group: &MessageGroup<Self::Message>,
        now: Timestamp,
    ) -> RetirementDecision {
        let age = now - group.first().received_at;
        if age >= self.max_group_age {
            RetirementDecision::Retire
        } else {
            RetirementDecision::NextCheckAt(group.first().received_at + self.max_group_age)
        }
    }
}

pub struct MessageBatcher {
    sender: UnboundedSender<MeasurementGrouper>,
    mqtt_client: Arc<dyn MqttClient>,
    source_topic_filter: TopicFilter,
    batcher: Batcher<OwnedCollectdMessage>,
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
        let batcher = Batcher::new(
            Box::new(CollectdTimestampDeltaGroupingPolicy {
                delta: batching_window.num_seconds() as f64,
            }),
            Box::new(MaxGroupAgeRetirementPolicy {
                max_group_age: batching_window,
            }),
        );

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

        loop {
            let now = self.clock.now();
            let retire_groups_action = self.batcher.retire_groups(now);

            for retired_group in retire_groups_action.retired_groups.iter() {
                // Send the current batch to the batch processor
                let _ = group_messages(retired_group)
                    .map_err(|err| error!("Error while grouping the message batch: {}", err))
                    .and_then(|group| {
                        self.sender.send(group).map_err(|err| {
                            error!("Error while publishing a message batch: {}", err)
                        })
                    });
            }

            let next_notification_at = retire_groups_action
                .next_check_at
                .unwrap_or(now + chrono::Duration::hours(24));

            // To avoid negative durations (which is not supported by std::time::Duration), fall back
            // to use a small timeout of 20 ms. Also use at least 20ms to avoid very small timeouts
            // which might in the worst case end up in a busy loop.
            let delta = next_notification_at - now;
            let t20ms = std::time::Duration::from_millis(20);
            let next_notify_in = std::cmp::max(t20ms, delta.to_std().unwrap_or(t20ms));

            self.process_io(messages.as_mut(), next_notify_in).await;
        }
    }

    async fn process_io(
        &mut self,
        messages: &mut dyn MqttMessageStream,
        next_notify_in: std::time::Duration,
    ) {
        match tokio::time::timeout_at(
            tokio::time::Instant::now() + next_notify_in,
            messages.next(),
        )
        .await
        {
            Err(_) => {
                // Timeout fired.
            }
            Ok(Some(mqtt_message)) => {
                // got a message
                let received_at = self.clock.now();
                match CollectdMessage::parse_from(&mqtt_message) {
                    Ok(collectd_message) => {
                        self.batcher.add_message(Envelope {
                            received_at,
                            message: collectd_message.into(),
                        });
                    }
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
    message_batch: &MessageGroup<OwnedCollectdMessage>,
) -> Result<MeasurementGrouper, DeviceMonitorError> {
    let mut message_grouper = MeasurementGrouper::new();
    message_grouper.timestamp(&message_batch.first().received_at)?;

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
    use clock::WallClock;
    use futures::future::{pending, ready};
    use mockall::Sequence;
    use mqtt_client::MockMqttClient;
    use mqtt_client::MockMqttMessageStream;
    use mqtt_client::QoS;
    use std::time::Duration;
    use tokio::time::{self, Instant};

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
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mut mqtt_client = MockMqttClient::new();
        mqtt_client.expect_subscribe().returning(|_| {
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

            Ok(Box::new(message_stream))
        });

        let clock = Arc::new(WallClock);
        let batcher = MessageBatcher::new(
            sender,
            Arc::new(mqtt_client),
            chrono::Duration::milliseconds(500),
            TopicFilter::new("collectd/#")?.qos(QoS::AtMostOnce),
            clock.clone(),
        );

        let _batcher_tid = tokio::task::spawn(async move {
            let mut batcher = batcher;
            let _ = batcher.run().await;
        });

        let message_grouper = receiver.recv().await.unwrap();

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
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mut mqtt_client = MockMqttClient::new();
        mqtt_client.expect_subscribe().returning(|_| {
            let message_stream = build_message_stream_from_messages(vec![
                ("collectd/localhost/temperature/value", "123456789:32.5"),
                ("collectd/pressure/value", "123456789:98.0"), // Erraneous collectd message with invalid topic
                ("collectd/localhost/speed/value", "123456789:350.0"),
            ]);
            Ok(Box::new(message_stream))
        });

        let clock = WallClock;
        let batcher = MessageBatcher::new(
            sender,
            Arc::new(mqtt_client),
            chrono::Duration::milliseconds(1000),
            TopicFilter::new("collectd/#")?.qos(QoS::AtMostOnce),
            Arc::new(clock.clone()),
        );

        let _batcher_tid = tokio::task::spawn(async move {
            let mut batcher = batcher;
            let _ = batcher.run().await;
        });

        let message_grouper = receiver.recv().await.unwrap();

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
    async fn batching_with_erraneous_first_message_does_not_send_a_batch() -> anyhow::Result<()> {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<MeasurementGrouper>();

        let mut mqtt_client = MockMqttClient::new();
        mqtt_client.expect_subscribe().returning(|_| {
            let message_stream = build_message_stream_from_messages(vec![
                ("collectd/host/group/key", "123456789"), // Invalid payload
            ]);
            Ok(Box::new(message_stream))
        });

        let clock = Arc::new(WallClock);
        let batcher = MessageBatcher::new(
            sender,
            Arc::new(mqtt_client),
            chrono::Duration::milliseconds(1000),
            TopicFilter::new("collectd/#")?.qos(QoS::AtMostOnce),
            clock.clone(),
        );

        let _batcher_tid = tokio::task::spawn(async move {
            let mut batcher = batcher;
            let _ = batcher.run().await;
        });

        // The batching window should close after 1000 ms.
        let result: Result<_, tokio::time::error::Elapsed> =
            tokio::time::timeout(Duration::from_millis(2000), receiver.recv()).await;

        assert!(result.is_err());

        Ok(())
    }

    fn build_message_stream_from_messages(
        message_map: Vec<(&'static str, &'static str)>,
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
                    let message = Message::new(&topic, format!("{}", message.1));
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
