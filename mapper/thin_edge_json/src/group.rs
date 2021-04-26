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

    mock! {
        pub GroupedVisitor {
        }

        impl GroupedMeasurementVisitor for GroupedVisitor {
            type Error = ();

            fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), ()>;
            fn measurement(&mut self, name: &str, value: f64) -> Result<(), ()>;
            fn start_group(&mut self, group: &str) -> Result<(), ()>;
            fn end_group(&mut self) -> Result<(), ()>;
        }
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

    fn test_timestamp(minute: u32) -> DateTime<FixedOffset> {
        FixedOffset::east(5 * 3600)
            .ymd(2021, 04, 08)
            .and_hms(13, minute, 00)
    }
}
