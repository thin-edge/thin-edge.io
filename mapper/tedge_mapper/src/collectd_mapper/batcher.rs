use clock::Timestamp;
use mqtt_client::Payload;
use thin_edge_json::{
    group::{MeasurementGroup, MeasurementGrouper},
    measurement::MeasurementVisitor,
    serialize::ThinEdgeJsonSerializer,
};

use crate::collectd_mapper::{collectd::CollectdMessage, error::DeviceMonitorError};
use chrono::Local;
use thin_edge_json::group::MeasurementGrouperError;

#[derive(Debug)]
pub struct MessageBatch {
    message_grouper: MeasurementGrouper,
}

impl MessageBatch {
    pub fn thin_edge_json_bytes(
        messages: Vec<CollectdMessage>,
    ) -> Result<Payload, DeviceMonitorError> {
        let mut messages = messages.into_iter();

        if let Some(first_message) = messages.next() {
            let timestamp = first_message.timestamp.with_timezone(Local::now().offset());
            let mut batch = MessageBatch::start_batch(first_message, timestamp)?;
            for message in messages {
                batch.add_to_batch(message)?;
            }
            let measurements = batch.end_batch()?;

            let mut tedge_json_serializer = ThinEdgeJsonSerializer::new();
            measurements.accept(&mut tedge_json_serializer)?;

            let payload = tedge_json_serializer.bytes()?;
            Ok(payload)
        } else {
            Err(DeviceMonitorError::InvalidThinEdgeJsonError(
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
    use chrono::{TimeZone, Utc};
    use clock::{Clock, WallClock};

    #[test]
    fn test_message_batch_processor() -> anyhow::Result<()> {
        let timestamp = Utc.ymd(2015, 5, 15).and_hms_milli(0, 0, 1, 444);
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
