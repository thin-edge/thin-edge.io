use chrono::offset::FixedOffset;
use chrono::DateTime;
use std::collections::HashMap;

use crate::measurement::{FlatMeasurementVisitor, GroupedMeasurementVisitor};

#[derive(Debug)]
pub struct MeasurementGrouper {
    pub timestamp: Option<DateTime<FixedOffset>>,
    pub values: HashMap<String, Measurement>,
}
#[derive(Debug)]
pub enum Measurement {
    Single(f64),
    Multi(HashMap<String, f64>),
}

#[derive(thiserror::Error, Debug)]
pub enum MeasurementGrouperError {
    #[error("Duplicated measurement: {0}")]
    DuplicatedMeasurement(String),

    #[error("Duplicated measurement: {0}.{1}")]
    DuplicatedSubMeasurement(String, String),
}

impl MeasurementGrouper {
    pub fn new() -> Self {
        Self {
            timestamp: None,
            values: HashMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn get_measurement_value(
        &self,
        group_key: Option<&str>,
        measurement_key: &str,
    ) -> Option<f64> {
        match group_key {
            Some(group_key) => match self.values.get(group_key) {
                Some(Measurement::Multi(map)) => map.get(measurement_key).cloned(), //hippo can we avoid this clone?
                _ => None,
            },
            None => match self.values.get(measurement_key) {
                Some(Measurement::Single(val)) => Some(*val),
                _ => None,
            },
        }
    }

    pub fn accept<V, E>(&self, visitor: &mut V) -> Result<(), E>
    where
        V: GroupedMeasurementVisitor<Error = E>,
    {
        if let Some(timestamp) = self.timestamp {
            visitor.timestamp(timestamp)?;
        }

        for (key, value) in self.values.iter() {
            match value {
                Measurement::Single(sv) => {
                    visitor.measurement(key, *sv)?;
                }
                Measurement::Multi(m) => {
                    visitor.start_group(key)?;
                    for (key, value) in m.iter() {
                        visitor.measurement(key, *value)?;
                    }
                    visitor.end_group()?;
                }
            }
        }
        Ok(())
    }
}

impl FlatMeasurementVisitor for MeasurementGrouper {
    type Error = MeasurementGrouperError;

    fn timestamp(&mut self, time: &DateTime<FixedOffset>) -> Result<(), Self::Error> {
        self.timestamp = Some(*time);
        Ok(())
    }

    fn measurement(
        &mut self,
        group: Option<&str>,
        name: &str,
        value: f64,
    ) -> Result<(), Self::Error> {
        let key = name.to_owned();

        match group {
            None => {
                self.values.insert(key, Measurement::Single(value));
                Ok(())
            }
            Some(group) => {
                let group_key = group.to_owned();
                if let Measurement::Multi(group_map) = self
                    .values
                    .entry(group_key)
                    .or_insert_with(|| Measurement::Multi(HashMap::new()))
                {
                    group_map.insert(name.to_owned(), value);
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::prelude::*;
    use mockall::predicate::*;
    use mockall::*;

    #[derive(thiserror::Error, Debug, Clone)]
    pub enum TestError {
        #[error("test")]
        _Test,
    }

    mock! {
        pub GroupedVisitor {
        }

        impl GroupedMeasurementVisitor for GroupedVisitor {
            type Error = TestError;

            fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), TestError>;
            fn measurement(&mut self, name: &str, value: f64) -> Result<(), TestError>;
            fn start_group(&mut self, group: &str) -> Result<(), TestError>;
            fn end_group(&mut self) -> Result<(), TestError>;
        }
    }

    #[test]
    fn new_measurement_grouper_is_empty() {
        let grouper = MeasurementGrouper::new();
        assert!(grouper.is_empty());

        let mut mock = MockGroupedVisitor::new();
        mock.expect_measurement().never();
        mock.expect_start_group().never();
        mock.expect_end_group().never();

        let _ = grouper.accept(&mut mock);
    }

    #[test]
    fn new_measurement_grouper_with_a_timestamp_is_empty() {
        let mut grouper = MeasurementGrouper::new();
        let _ = grouper.timestamp(&test_timestamp(4));
        assert!(grouper.is_empty());

        let mut mock = MockGroupedVisitor::new();
        mock.expect_timestamp().return_const(Ok(()));
        mock.expect_measurement().never();
        mock.expect_start_group().never();
        mock.expect_end_group().never();

        let _ = grouper.accept(&mut mock);
    }

    #[test]
    fn new_measurement_grouper_has_no_timestamp() {
        let grouper = MeasurementGrouper::new();
        let mut mock = MockGroupedVisitor::new();

        mock.expect_timestamp().never();

        let _ = grouper.accept(&mut mock);
    }

    #[test]
    fn measurement_grouper_forward_timestamp() {
        let mut grouper = MeasurementGrouper::new();
        let _ = grouper.timestamp(&test_timestamp(4));

        let mut mock = MockGroupedVisitor::new();
        mock.expect_timestamp()
            .times(1)
            .with(eq(test_timestamp(4)))
            .return_const(Ok(()));

        let _ = grouper.accept(&mut mock);
    }

    #[test]
    fn measurement_grouper_forward_only_the_latest_received_timestamp() {
        let mut grouper = MeasurementGrouper::new();
        let _ = grouper.timestamp(&test_timestamp(4));
        let _ = grouper.timestamp(&test_timestamp(6));
        let _ = grouper.timestamp(&test_timestamp(5));

        let mut mock = MockGroupedVisitor::new();
        mock.expect_timestamp()
            .times(1)
            .with(eq(test_timestamp(5)))
            .return_const(Ok(()));

        let _ = grouper.accept(&mut mock);
    }

    #[test]
    fn get_measurement_value() -> anyhow::Result<()> {
        let mut grouper = MeasurementGrouper::new();
        grouper.measurement(None, "temperature", 32.5)?;
        grouper.measurement(Some("coordinate"), "x", 50.0)?;
        grouper.measurement(Some("coordinate"), "y", 70.0)?;
        grouper.measurement(Some("coordinate"), "z", 90.0)?;
        grouper.measurement(None, "pressure", 98.2)?;

        assert_eq!(
            grouper.get_measurement_value(None, "temperature").unwrap(),
            32.5
        );
        assert_eq!(
            grouper.get_measurement_value(None, "pressure").unwrap(),
            98.2
        );
        assert_eq!(
            grouper
                .get_measurement_value(Some("coordinate"), "x")
                .unwrap(),
            50.0
        );
        assert_eq!(
            grouper
                .get_measurement_value(Some("coordinate"), "y")
                .unwrap(),
            70.0
        );
        assert_eq!(
            grouper
                .get_measurement_value(Some("coordinate"), "z")
                .unwrap(),
            90.0
        );

        Ok(())
    }

    fn test_timestamp(minute: u32) -> DateTime<FixedOffset> {
        FixedOffset::east(5 * 3600)
            .ymd(2021, 4, 8)
            .and_hms(13, minute, 00)
    }
}
