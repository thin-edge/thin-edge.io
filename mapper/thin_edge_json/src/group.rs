use chrono::offset::FixedOffset;
use chrono::DateTime;
use std::collections::HashMap;

use crate::measurement::{FlatMeasurementVisitor, GroupedMeasurementVisitor};
use crate::serialize::ThinEdgeJsonSerializationError;

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

    #[error("Visitor Error")]
    ThinEdgeJsonSerializationError(#[from] ThinEdgeJsonSerializationError),
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

    fn timestamp(&mut self, timestamp: &DateTime<FixedOffset>) -> Result<(), Self::Error> {
        self.timestamp = *timestamp;
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
                return Ok(());
            }
            Some(group) => {
                let group_key = group.to_owned();

                match self
                    .values
                    .entry(group_key)
                    .or_insert_with(|| Measurement::Multi(HashMap::new()))
                {
                    Measurement::Multi(group_map) => {
                        group_map.insert(name.to_owned(), value);
                    }

                    Measurement::Single(_) => {
                        return Err(MeasurementGrouperError::DuplicatedMeasurement(key));
                    }
                }
                Ok(())
            }
        }
    }
}
