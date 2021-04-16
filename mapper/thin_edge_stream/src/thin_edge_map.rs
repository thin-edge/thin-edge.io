use crate::measurement::GroupedMeasurementConsumer;
use crate::measurement::MeasurementConsumer;
use chrono::{DateTime, FixedOffset};
use std::collections::HashMap;

pub struct ThinEdgeJsonMap {
    pub timestamp: DateTime<FixedOffset>,
    pub values: HashMap<String, Measurement>,
}

pub enum Measurement {
    Single(f64),
    Multi(HashMap<String, f64>),
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeJsonMapError {
    #[error("Duplicated measurement: {0}")]
    DuplicatedMeasurement(String),

    #[error("Duplicated measurement: {0}.{1}")]
    DuplicatedSubMeasurement(String, String),
}

pub struct ThinEdgeJsonMapBuilder {
    data: ThinEdgeJsonMap,
}

impl ThinEdgeJsonMapBuilder {
    pub fn new(timestamp: DateTime<FixedOffset>) -> ThinEdgeJsonMapBuilder {
        let data = ThinEdgeJsonMap {
            timestamp,
            values: HashMap::new(),
        };
        ThinEdgeJsonMapBuilder { data }
    }
}

impl GroupedMeasurementConsumer for ThinEdgeJsonMapBuilder {
    type Error = ThinEdgeJsonMapError;
    type Data = ThinEdgeJsonMap;

    fn start(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn end(self) -> Result<ThinEdgeJsonMap, Self::Error> {
        Ok(self.data)
    }

    fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        self.data.timestamp = value;
        Ok(())
    }

    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error> {
        let key = name.to_owned();

        if self.data.values.contains_key(&key) {
            return Err(ThinEdgeJsonMapError::DuplicatedMeasurement(key));
        }

        self.data.values.insert(key, Measurement::Single(value));
        Ok(())
    }

    fn start_group(&mut self, name: &str) -> Result<(), Self::Error> {
        let key = name.to_owned();

        match self.data.values.get(&key) {
            None => {
                let group = Measurement::Multi(HashMap::new());
                self.data.values.insert(key, group);
                Ok(())
            }
            Some(Measurement::Multi(_)) => {
                // group already created
                Ok(())
            }
            Some(Measurement::Single(_)) => Err(ThinEdgeJsonMapError::DuplicatedMeasurement(key)),
        }
    }

    fn end_group(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl MeasurementConsumer for ThinEdgeJsonMapBuilder {
    type Error = ThinEdgeJsonMapError;
    type Data = ThinEdgeJsonMap;

    fn start(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn end(self) -> Result<ThinEdgeJsonMap, Self::Error> {
        Ok(self.data)
    }

    fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        self.data.timestamp = value;
        Ok(())
    }

    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error> {
        let key = name.to_owned();

        if self.data.values.contains_key(&key) {
            return Err(ThinEdgeJsonMapError::DuplicatedMeasurement(key));
        }

        self.data.values.insert(key, Measurement::Single(value));
        Ok(())
    }

    fn sub_measurement(&mut self, group: &str, name: &str, value: f64) -> Result<(), Self::Error> {
        let key = group.to_owned();

        if !self.data.values.contains_key(&key) {
            self.data
                .values
                .insert(key.clone(), Measurement::Multi(HashMap::new()));
        }

        let group = match self.data.values.get_mut(&key) {
            Some(Measurement::Multi(group)) => group,
            _ => {
                return Err(ThinEdgeJsonMapError::DuplicatedMeasurement(key));
            }
        };

        let sub_key = name.to_owned();
        if group.contains_key(&sub_key) {
            return Err(ThinEdgeJsonMapError::DuplicatedSubMeasurement(key, sub_key));
        }

        group.insert(sub_key, value);
        Ok(())
    }
}
