use chrono::offset::FixedOffset;
use chrono::DateTime;
use std::collections::HashMap;

use crate::measurement::{FlatMeasurementVisitor, GroupedMeasurementVisitor};
#[derive(Debug)]
pub struct MeasurementMap {
    pub timestamp: DateTime<FixedOffset>,
    pub values: HashMap<String, Measurement>,
}
#[derive(Debug)]
pub enum Measurement {
    Single(f64),
    Multi(HashMap<String, f64>),
}

#[derive(thiserror::Error, Debug)]
pub enum MeasurementMapError {
    #[error("Duplicated measurement: {0}")]
    DuplicatedMeasurement(String),

    #[error("Duplicated measurement: {0}.{1}")]
    DuplicatedSubMeasurement(String, String),
}

impl MeasurementMap {
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
        visitor.timestamp(self.timestamp.clone())?;
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

impl FlatMeasurementVisitor for MeasurementMap {
    type Error = MeasurementMapError;

    fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        self.timestamp = value;
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
                let key = group.to_owned();

                if !self.values.contains_key(&key) {
                    self.values
                        .insert(key.clone(), Measurement::Multi(HashMap::new()));
                }

                let group = match self.values.get_mut(&key) {
                    Some(Measurement::Multi(group)) => group,
                    _ => {
                        return Err(MeasurementMapError::DuplicatedMeasurement(key));
                    }
                };

                let sub_key = name.to_owned();
                group.insert(sub_key, value);
                Ok(())
            }
        }
    }
}
