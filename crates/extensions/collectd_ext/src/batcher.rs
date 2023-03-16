use clock::Timestamp;
use tedge_api::group::MeasurementGroup;
use tedge_api::group::MeasurementGrouper;
use tedge_api::group::MeasurementGrouperError;
use tedge_api::measurement::MeasurementVisitor;
use tedge_api::serialize::ThinEdgeJsonSerializer;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;

use super::collectd::CollectdMessage;
use super::error::DeviceMonitorError;

#[derive(Debug)]
pub struct MessageBatch {
    message_grouper: MeasurementGrouper,
}

impl MessageBatch {
    pub fn thin_edge_json(
        output_topic: &Topic,
        messages: Vec<CollectdMessage>,
    ) -> Result<MqttMessage, DeviceMonitorError> {
        let mut messages = messages.into_iter();

        if let Some(first_message) = messages.next() {
            let timestamp = first_message.timestamp;
            let mut batch = MessageBatch::start_batch(first_message, timestamp)?;
            for message in messages {
                batch.add_to_batch(message)?;
            }
            let measurements = batch.end_batch()?;

            let mut tedge_json_serializer = ThinEdgeJsonSerializer::new();
            measurements.accept(&mut tedge_json_serializer)?;

            let payload = tedge_json_serializer.bytes()?;
            Ok(MqttMessage::new(output_topic, payload))
        } else {
            Err(DeviceMonitorError::FromInvalidThinEdgeJson(
                MeasurementGrouperError::UnexpectedEnd,
            ))
        }
    }

    fn start_batch(
        collectd_message: CollectdMessage,
        timestamp: Timestamp,
    ) -> Result<Self, DeviceMonitorError> {
        let mut message_grouper = MeasurementGrouper::new();
        message_grouper.visit_timestamp(timestamp)?;

        let mut message_batch = Self { message_grouper };

        message_batch.add_to_batch(collectd_message)?;

        Ok(message_batch)
    }

    fn add_to_batch(
        &mut self,
        collectd_message: CollectdMessage,
    ) -> Result<(), DeviceMonitorError> {
        collectd_message.accept(&mut self.message_grouper)?;
        Ok(())
    }

    fn end_batch(self) -> Result<MeasurementGroup, DeviceMonitorError> {
        Ok(self.message_grouper.end()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use clock::Clock;
    use clock::WallClock;
    use time::macros::datetime;

    #[test]
    fn test_message_batch_processor() -> anyhow::Result<()> {
        let timestamp = datetime!(2015-05-15 0:00:01.444 UTC);
        let collectd_message = CollectdMessage::new("temperature", "value", 32.5, timestamp);
        let mut message_batch = MessageBatch::start_batch(collectd_message, WallClock.now())?;

        let collectd_message = CollectdMessage::new("coordinate", "x", 50.0, timestamp);
        message_batch.add_to_batch(collectd_message)?;

        let collectd_message = CollectdMessage::new("coordinate", "y", 70.0, timestamp);
        message_batch.add_to_batch(collectd_message)?;

        let collectd_message = CollectdMessage::new("pressure", "value", 98.2, timestamp);
        message_batch.add_to_batch(collectd_message)?;

        let collectd_message = CollectdMessage::new("coordinate", "z", 90.0, timestamp);
        message_batch.add_to_batch(collectd_message)?;

        let message_group = message_batch.end_batch()?;

        assert_matches!(message_group.timestamp(), Some(_));

        assert_eq!(
            message_group.get_measurement_value(Some("temperature"), "value"),
            Some(32.5)
        );
        assert_eq!(
            message_group.get_measurement_value(Some("pressure"), "value"),
            Some(98.2)
        );
        assert_eq!(
            message_group.get_measurement_value(Some("coordinate"), "x"),
            Some(50.0)
        );
        assert_eq!(
            message_group.get_measurement_value(Some("coordinate"), "y"),
            Some(70.0)
        );
        assert_eq!(
            message_group.get_measurement_value(Some("coordinate"), "z"),
            Some(90.0)
        );

        Ok(())
    }
}
