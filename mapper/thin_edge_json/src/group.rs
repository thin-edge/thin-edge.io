use chrono::offset::FixedOffset;
use chrono::DateTime;
use std::collections::HashMap;

use crate::measurement::{FlatMeasurementVisitor, GroupedMeasurementVisitor};

#[derive(Debug)]
pub struct MeasurementGrouper {
    pub timestamp: DateTime<FixedOffset>,
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
    pub fn new(timestamp: DateTime<FixedOffset>) -> Self {
        Self {
            timestamp,
            values: HashMap::new(),
        }
    }

    pub fn accept<V, E>(&self, visitor: &mut V) -> Result<(), E>
    where
        V: GroupedMeasurementVisitor<Error = E>,
    {
        visitor.timestamp(self.timestamp)?;
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
        self.timestamp = *time;
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
