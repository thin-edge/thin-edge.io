use crate::{data::*, json::ThinEdgeJsonError, measurement::*};
use chrono::prelude::*;

/// A `MeasurementVisitor` that builds up `ThinEdgeJson`.
pub struct ThinEdgeJsonBuilder {
    timestamp: Option<DateTime<FixedOffset>>,
    inside_group: Option<MultiValueMeasurement>,
    measurements: Vec<ThinEdgeValue>,
}

impl ThinEdgeJsonBuilder {
    pub fn new() -> Self {
        Self {
            timestamp: None,
            inside_group: None,
            measurements: Vec::new(),
        }
    }

    pub fn done(self) -> Result<ThinEdgeJson, ThinEdgeJsonError> {
        if self.inside_group.is_some() {
            return Err(ThinEdgeJsonError::UnexpectedOpenGroup);
        }

        if self.measurements.is_empty() {
            return Err(ThinEdgeJsonError::EmptyThinEdgeJsonRoot);
        }

        Ok(ThinEdgeJson {
            timestamp: self.timestamp,
            values: self.measurements,
        })
    }
}

impl MeasurementVisitor for ThinEdgeJsonBuilder {
    type Error = ThinEdgeJsonError;

    fn visit_timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        match self.timestamp {
            None => {
                self.timestamp = Some(value);
                Ok(())
            }
            Some(_) => Err(ThinEdgeJsonError::DuplicatedTimestamp),
        }
    }

    fn visit_measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error> {
        if let Some(group) = &mut self.inside_group {
            group.values.push((name, value).into());
        } else {
            self.measurements.push((name, value).into());
        }
        Ok(())
    }

    fn visit_start_group(&mut self, group: &str) -> Result<(), Self::Error> {
        if self.inside_group.is_none() {
            self.inside_group = Some(MultiValueMeasurement {
                name: group.into(),
                values: Vec::new(),
            });
            Ok(())
        } else {
            Err(ThinEdgeJsonError::UnexpectedStartOfGroup)
        }
    }

    fn visit_end_group(&mut self) -> Result<(), Self::Error> {
        match self.inside_group.take() {
            Some(group) => {
                if group.values.is_empty() {
                    return Err(ThinEdgeJsonError::EmptyThinEdgeJson { name: group.name });
                } else {
                    self.measurements.push(ThinEdgeValue::Multi(group))
                }
            }
            None => return Err(ThinEdgeJsonError::UnexpectedEndOfGroup),
        }
        Ok(())
    }
}
