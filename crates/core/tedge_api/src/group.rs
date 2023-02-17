use std::collections::HashMap;
use time::OffsetDateTime;

use crate::measurement::MeasurementVisitor;

#[derive(Debug)]
pub struct MeasurementGroup {
    timestamp: Option<OffsetDateTime>,
    values: HashMap<String, Measurement>,
}

impl MeasurementGroup {
    fn new() -> Self {
        Self {
            timestamp: None,
            values: HashMap::new(),
        }
    }

    pub fn timestamp(&self) -> Option<OffsetDateTime> {
        self.timestamp
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
                Some(Measurement::Multi(map)) => map.get(measurement_key).cloned(),
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
        V: MeasurementVisitor<Error = E>,
        E: std::error::Error + std::fmt::Debug,
    {
        if let Some(timestamp) = self.timestamp {
            visitor.visit_timestamp(timestamp)?;
        }

        for (key, value) in self.values.iter() {
            match value {
                Measurement::Single(sv) => {
                    visitor.visit_measurement(key, *sv)?;
                }
                Measurement::Multi(m) => {
                    visitor.visit_start_group(key)?;
                    for (key, value) in m.iter() {
                        visitor.visit_measurement(key, *value)?;
                    }
                    visitor.visit_end_group()?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct MeasurementGrouper {
    measurement_group: MeasurementGroup,
    group_state: GroupState,
}

/// Keeps track whether we are currently in a group or not.
/// This serves the same purpose an `Option<String>` would do, just that
/// the `String` is not allocated over and over again.
#[derive(Debug)]
struct GroupState {
    in_group: bool,
    group: String,
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

    #[error("Unexpected end")]
    UnexpectedEnd,

    #[error("Unexpected start of group")]
    UnexpectedStartOfGroup,

    #[error("Unexpected end of group")]
    UnexpectedEndOfGroup,
}

impl MeasurementGrouper {
    pub fn new() -> Self {
        Self {
            measurement_group: MeasurementGroup::new(),
            group_state: GroupState {
                in_group: false,
                group: String::with_capacity(20),
            },
        }
    }

    pub fn end(self) -> Result<MeasurementGroup, MeasurementGrouperError> {
        if self.group_state.in_group {
            Err(MeasurementGrouperError::UnexpectedEnd)
        } else {
            Ok(self.measurement_group)
        }
    }
}

impl Default for MeasurementGrouper {
    fn default() -> Self {
        Self::new()
    }
}

impl MeasurementVisitor for MeasurementGrouper {
    type Error = MeasurementGrouperError;

    fn visit_timestamp(&mut self, time: OffsetDateTime) -> Result<(), Self::Error> {
        self.measurement_group.timestamp = Some(time);
        Ok(())
    }

    fn visit_start_group(&mut self, group: &str) -> Result<(), Self::Error> {
        if self.group_state.in_group {
            Err(MeasurementGrouperError::UnexpectedStartOfGroup)
        } else {
            self.group_state.in_group = true;
            self.group_state.group.replace_range(.., group);
            Ok(())
        }
    }

    fn visit_end_group(&mut self) -> Result<(), Self::Error> {
        if self.group_state.in_group {
            self.group_state.in_group = false;
            self.group_state.group.clear();
            Ok(())
        } else {
            Err(MeasurementGrouperError::UnexpectedEndOfGroup)
        }
    }

    fn visit_measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error> {
        let key = name.to_owned();

        match self.group_state.in_group {
            false => {
                self.measurement_group
                    .values
                    .insert(key, Measurement::Single(value));
                Ok(())
            }
            true => {
                let group_key = self.group_state.group.clone();
                if let Measurement::Multi(group_map) = self
                    .measurement_group
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
    use mockall::predicate::*;
    use mockall::*;
    use time::macros::datetime;
    use time::Duration;

    #[derive(thiserror::Error, Debug, Clone)]
    pub enum TestError {
        #[error("test")]
        _Test,
    }

    mock! {
        pub GroupedVisitor {
        }

        impl MeasurementVisitor for GroupedVisitor {
            type Error = TestError;

            fn visit_timestamp(&mut self, value: OffsetDateTime) -> Result<(), TestError>;
            fn visit_measurement(&mut self, name: &str, value: f64) -> Result<(), TestError>;
            fn visit_start_group(&mut self, group: &str) -> Result<(), TestError>;
            fn visit_end_group(&mut self) -> Result<(), TestError>;
        }
    }

    // XXX: These test cases should be split into those test cases that test the MeasurementGrouper and
    // those that test the MeasurementGroup.
    #[test]
    fn new_measurement_grouper_is_empty() -> anyhow::Result<()> {
        let grouper = MeasurementGrouper::new();
        let group = grouper.end()?;
        assert!(group.is_empty());

        Ok(())
    }

    #[test]
    fn empty_measurement_group_visits_nothing() -> anyhow::Result<()> {
        let group = MeasurementGroup::new();

        let mut mock = MockGroupedVisitor::new();
        mock.expect_visit_measurement().never();
        mock.expect_visit_start_group().never();
        mock.expect_visit_end_group().never();

        group.accept(&mut mock)?;

        Ok(())
    }

    #[test]
    fn new_measurement_grouper_with_a_timestamp_is_empty() -> anyhow::Result<()> {
        let mut grouper = MeasurementGrouper::new();
        let _ = grouper.visit_timestamp(test_timestamp(4));

        let group = grouper.end()?;
        assert!(group.is_empty());

        let mut mock = MockGroupedVisitor::new();
        mock.expect_visit_timestamp().return_const(Ok(()));
        mock.expect_visit_measurement().never();
        mock.expect_visit_start_group().never();
        mock.expect_visit_end_group().never();

        let _ = group.accept(&mut mock);

        Ok(())
    }

    #[test]
    fn new_measurement_grouper_has_no_timestamp() -> anyhow::Result<()> {
        let grouper = MeasurementGrouper::new();
        let mut mock = MockGroupedVisitor::new();

        mock.expect_visit_timestamp().never();
        let group = grouper.end()?;
        let _ = group.accept(&mut mock);

        Ok(())
    }

    #[test]
    fn measurement_grouper_forward_timestamp() -> anyhow::Result<()> {
        let mut grouper = MeasurementGrouper::new();
        let _ = grouper.visit_timestamp(test_timestamp(4));

        let mut mock = MockGroupedVisitor::new();
        mock.expect_visit_timestamp()
            .times(1)
            .with(eq(test_timestamp(4)))
            .return_const(Ok(()));

        let group = grouper.end()?;
        let _ = group.accept(&mut mock);

        Ok(())
    }

    #[test]
    fn measurement_grouper_forward_only_the_latest_received_timestamp() -> anyhow::Result<()> {
        let mut grouper = MeasurementGrouper::new();
        let _ = grouper.visit_timestamp(test_timestamp(4));
        let _ = grouper.visit_timestamp(test_timestamp(6));
        let _ = grouper.visit_timestamp(test_timestamp(5));

        let mut mock = MockGroupedVisitor::new();
        mock.expect_visit_timestamp()
            .times(1)
            .with(eq(test_timestamp(5)))
            .return_const(Ok(()));

        let group = grouper.end()?;
        let _ = group.accept(&mut mock);

        Ok(())
    }

    #[test]
    fn get_measurement_value() -> anyhow::Result<()> {
        let mut grouper = MeasurementGrouper::new();
        grouper.visit_measurement("temperature", 32.5)?;
        grouper.visit_start_group("coordinate")?;
        grouper.visit_measurement("x", 50.0)?;
        grouper.visit_measurement("y", 70.0)?;
        grouper.visit_measurement("z", 90.0)?;
        grouper.visit_end_group()?;
        grouper.visit_measurement("pressure", 98.2)?;

        let group = grouper.end()?;

        assert_eq!(
            group.get_measurement_value(None, "temperature").unwrap(),
            32.5
        );
        assert_eq!(group.get_measurement_value(None, "pressure").unwrap(), 98.2);
        assert_eq!(
            group
                .get_measurement_value(Some("coordinate"), "x")
                .unwrap(),
            50.0
        );
        assert_eq!(
            group
                .get_measurement_value(Some("coordinate"), "y")
                .unwrap(),
            70.0
        );
        assert_eq!(
            group
                .get_measurement_value(Some("coordinate"), "z")
                .unwrap(),
            90.0
        );

        Ok(())
    }

    fn test_timestamp(minute: u32) -> OffsetDateTime {
        let mut dt = datetime!(2021-04-08 13:00:00 +05:00);
        dt += Duration::minutes(minute as i64);
        dt
    }
}
