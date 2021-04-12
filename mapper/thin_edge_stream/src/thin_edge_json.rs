use chrono::{DateTime, FixedOffset};
use crate::builder::GroupedMeasurementCollector;
use crate::builder::GroupedMeasurementBuilder;
use crate::builder::MeasurementCollectorError;

pub struct ThinEdgeJson {
    pub timestamp: DateTime<FixedOffset>,
    pub values: Vec<ThinEdgeValue>,
}

pub enum ThinEdgeValue {
    Single(SingleValueMeasurement),
    Multi(MultiValueMeasurement),
}

pub struct SingleValueMeasurement {
    pub name: String,
    pub value: f64,
}

pub struct MultiValueMeasurement {
    pub name: String,
    pub values: Vec<SingleValueMeasurement>,
}

impl GroupedMeasurementBuilder for ThinEdgeJson {
    fn build<C,E,D>(&self, mut collector: C) -> Result<D,E>
        where C : GroupedMeasurementCollector<Error = E, Data = D>
    {
        collector.start()?;
        collector.timestamp(self.timestamp)?;

        for value in self.values.iter() {
            match value {
                ThinEdgeValue::Single(ref measurement) => {
                    collector.measurement(&measurement.name, measurement.value)?;
                },
                ThinEdgeValue::Multi(group) => {
                    collector.start_group(&group.name)?;
                    for sub_measurement in group.values.iter() {
                        collector.measurement(&sub_measurement.name, sub_measurement.value)?;
                    }
                    collector.end_group()?;
                }
            }
        }

        collector.end()
    }
}

pub struct ThinEdgeJsonBuilder {
    data: ThinEdgeJson,
    group: Option<MultiValueMeasurement>,
}

impl ThinEdgeJsonBuilder {
    pub fn new(timestamp: DateTime<FixedOffset>) -> ThinEdgeJsonBuilder {
        let data = ThinEdgeJson { timestamp, values: vec![] };
        ThinEdgeJsonBuilder { data , group: None }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeJsonBuilderError {
    #[error(transparent)]
    MeasurementCollectorError(#[from] MeasurementCollectorError),
}

impl GroupedMeasurementCollector for ThinEdgeJsonBuilder {
    type Error = ThinEdgeJsonBuilderError;
    type Data = ThinEdgeJson;

    fn start(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn end(self) -> Result<ThinEdgeJson, Self::Error> {
        Ok(self.data)
    }

    fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        self.data.timestamp = value;
        Ok(())
    }

    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error> {
        let item = ThinEdgeValue::Single(SingleValueMeasurement {
            name: name.to_owned(),
            value,
        });
        self.data.values.push(item);
        Ok(())
    }

    fn start_group(&mut self, name: &str) -> Result<(), Self::Error> {
        match self.group {
            Some(_) => Err(MeasurementCollectorError::UnexpectedStartOfGroup.into()),
            None => {
                let group = MultiValueMeasurement { name: name.to_owned(), values: vec![] };
                self.group = Some(group);
                Ok(())
            }
        }
    }

    fn end_group(&mut self) -> Result<(), Self::Error> {
        match self.group.take() {
            None => Err(MeasurementCollectorError::UnexpectedEndOfGroup.into()),
            Some(group) => {
                let items = ThinEdgeValue::Multi(group);
                self.data.values.push(items);
                Ok(())
            }
        }
    }
}
