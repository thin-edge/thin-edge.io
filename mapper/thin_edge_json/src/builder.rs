use crate::{data::*, measurement::*};
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

    pub fn done(self) -> Result<ThinEdgeJson, ThinEdgeJsonBuilderError> {
        if self.inside_group.is_some() {
            return Err(ThinEdgeJsonBuilderError::UnexpectedOpenGroup);
        }

        if self.measurements.is_empty() {
            return Err(ThinEdgeJsonBuilderError::EmptyThinEdgeJsonRoot);
        }

        Ok(ThinEdgeJson {
            timestamp: self.timestamp,
            values: self.measurements,
        })
    }
}

impl MeasurementVisitor for ThinEdgeJsonBuilder {
    type Error = ThinEdgeJsonBuilderError;

    fn visit_timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        match self.timestamp {
            None => {
                self.timestamp = Some(value);
                Ok(())
            }
            Some(_) => Err(ThinEdgeJsonBuilderError::DuplicatedTimestamp),
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
            Err(ThinEdgeJsonBuilderError::UnexpectedStartOfGroup)
        }
    }

    fn visit_end_group(&mut self) -> Result<(), Self::Error> {
        match self.inside_group.take() {
            Some(group) => {
                if group.values.is_empty() {
                    return Err(ThinEdgeJsonBuilderError::EmptyThinEdgeJson { name: group.name });
                } else {
                    self.measurements.push(ThinEdgeValue::Multi(group))
                }
            }
            None => return Err(ThinEdgeJsonBuilderError::UnexpectedEndOfGroup),
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeJsonBuilderError {
    #[error("Empty Thin Edge measurement: it must contain at least one measurement")]
    EmptyThinEdgeJsonRoot,

    #[error("Empty Thin Edge measurement: {name:?} must contain at least one measurement")]
    EmptyThinEdgeJson { name: String },

    #[error("... time stamp within a group")]
    DuplicatedTimestamp,

    #[error("Unexpected open group")]
    UnexpectedOpenGroup,

    #[error("Unexpected start of group")]
    UnexpectedStartOfGroup,

    #[error("Unexpected end of group")]
    UnexpectedEndOfGroup,
}
