//! The in-memory data model representing ThinEdge JSON.

use time::OffsetDateTime;

/// In-memory representation of parsed ThinEdge JSON.
#[derive(Debug)]
pub struct ThinEdgeJson {
    pub timestamp: Option<OffsetDateTime>,
    pub values: Vec<ThinEdgeValue>,
}

impl ThinEdgeJson {
    pub fn has_timestamp(&self) -> bool {
        self.timestamp.is_some()
    }

    pub fn set_timestamp(&mut self, timestamp: OffsetDateTime) {
        self.timestamp = Some(timestamp)
    }
}

#[derive(Debug, PartialEq)]
pub enum ThinEdgeValue {
    Single(SingleValueMeasurement),
    Multi(MultiValueMeasurement),
}

#[derive(Debug, PartialEq)]
pub struct SingleValueMeasurement {
    pub name: String,
    pub value: f64,
}

#[derive(Debug, PartialEq)]
pub struct MultiValueMeasurement {
    pub name: String,
    pub values: Vec<SingleValueMeasurement>,
}

impl<T> From<(T, f64)> for SingleValueMeasurement
where
    T: Into<String>,
{
    fn from((name, value): (T, f64)) -> Self {
        SingleValueMeasurement {
            name: name.into(),
            value,
        }
    }
}

impl<T> From<(T, f64)> for ThinEdgeValue
where
    T: Into<String>,
{
    fn from((name, value): (T, f64)) -> Self {
        ThinEdgeValue::Single((name, value).into())
    }
}

impl<T> From<(T, Vec<SingleValueMeasurement>)> for ThinEdgeValue
where
    T: Into<String>,
{
    fn from((name, values): (T, Vec<SingleValueMeasurement>)) -> Self {
        ThinEdgeValue::Multi(MultiValueMeasurement {
            name: name.into(),
            values,
        })
    }
}
