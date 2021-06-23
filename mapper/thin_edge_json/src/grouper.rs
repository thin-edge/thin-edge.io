use crate::stream::*;
use chrono::{offset::FixedOffset, DateTime};
use std::collections::HashMap;

/// This will turn a MeasurementStream into a HashMap.
#[derive(Debug)]
pub struct MeasurementGrouper {
    pub timestamp: Option<DateTime<FixedOffset>>,
    pub values: HashMap<String, Measurement>,
    pub group_name: Option<String>,
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

    #[error("Nested groups: {0}.{1}")]
    NestedGroups(String, String),
}

impl MeasurementGrouper {
    pub fn new() -> Self {
        Self {
            timestamp: None,
            values: HashMap::new(),
            group_name: None,
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

    pub fn stream_to<C, E>(&self, sink: C) -> Result<C, E>
    where
        C: MeasurementStreamConsumer<Error = E>,
        E: std::error::Error + std::fmt::Debug,
    {
        let mut stream = StreamBuilder::from(sink);

        stream.start()?;

        if let Some(timestamp) = self.timestamp {
            stream.timestamp(timestamp)?;
        }

        for (key, value) in self.values.iter() {
            match value {
                Measurement::Single(sv) => {
                    stream.measurement(key, *sv)?;
                }
                Measurement::Multi(m) => {
                    stream.start_group(key)?;
                    for (key, value) in m.iter() {
                        stream.measurement(key, *value)?;
                    }
                    stream.end_group()?;
                }
            }
        }

        stream.end()?;
        Ok(stream.inner())
    }
}

impl MeasurementStreamConsumer for MeasurementGrouper {
    type Error = MeasurementGrouperError;

    fn consume<'a>(&mut self, item: MeasurementStreamItem<'a>) -> Result<(), Self::Error> {
        match item {
            MeasurementStreamItem::StartDocument => Ok(()),
            MeasurementStreamItem::EndDocument => Ok(()),
            MeasurementStreamItem::Timestamp(timestamp) => {
                self.timestamp = Some(timestamp);
                Ok(())
            }
            MeasurementStreamItem::StartGroup(group) => match self.group_name {
                None => {
                    self.group_name = Some(group.into());
                    Ok(())
                }
                Some(ref parent_group) => Err(MeasurementGrouperError::NestedGroups(
                    parent_group.clone(),
                    group.into(),
                )),
            },
            MeasurementStreamItem::EndGroup => {
                self.group_name = None;
                Ok(())
            }

            MeasurementStreamItem::Measurement { name, value } => match self.group_name {
                None => {
                    self.values.insert(name.into(), Measurement::Single(value));
                    Ok(())
                }
                Some(ref group_key) => {
                    if let Measurement::Multi(group_map) = self
                        .values
                        .entry(group_key.clone())
                        .or_insert_with(|| Measurement::Multi(HashMap::new()))
                    {
                        group_map.insert(name.to_owned(), value);
                    }
                    Ok(())
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_measurement_value() -> anyhow::Result<()> {
        let mut grouper = StreamBuilder::from(MeasurementGrouper::new());
        grouper.start()?;
        grouper.measurement("temperature", 32.5)?;
        grouper.measurement_in_group("coordinate", "x", 50.0)?;
        grouper.measurement_in_group("coordinate", "y", 70.0)?;
        grouper.measurement_in_group("coordinate", "z", 90.0)?;
        grouper.measurement("pressure", 98.2)?;
        grouper.end()?;
        let grouper = grouper.inner();

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
}
