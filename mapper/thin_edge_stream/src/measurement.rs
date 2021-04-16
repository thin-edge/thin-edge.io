use chrono::{DateTime, FixedOffset};

pub trait MeasurementConsumer {
    type Error;
    type Data;

    fn start(&mut self) -> Result<(), Self::Error>;
    fn end(self) -> Result<Self::Data, Self::Error>;

    fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error>;

    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error>;
    fn sub_measurement(&mut self, group: &str, name: &str, value: f64) -> Result<(), Self::Error>;
}

pub trait GroupedMeasurementConsumer {
    type Error;
    type Data;

    fn start(&mut self) -> Result<(), Self::Error>;
    fn end(self) -> Result<Self::Data, Self::Error>;

    fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error>;

    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error>;
    fn start_group(&mut self, group: &str) -> Result<(), Self::Error>;
    fn end_group(&mut self) -> Result<(), Self::Error>;
}

pub trait MeasurementProducer {
    fn build<C, E, D>(&self, collector: C) -> Result<D, E>
    where
        C: MeasurementConsumer<Error = E, Data = D>;
}

pub trait GroupedMeasurementProducer {
    fn build<C, E, D>(&self, collector: C) -> Result<D, E>
    where
        C: GroupedMeasurementConsumer<Error = E, Data = D>;
}

#[derive(thiserror::Error, Debug)]
pub enum MeasurementStreamError {
    #[error("Unexpected time stamp within a group")]
    UnexpectedTimestamp,

    #[error("Unexpected end of data")]
    UnexpectedEndOfData,

    #[error("Unexpected end of group")]
    UnexpectedEndOfGroup,

    #[error("Unexpected start of group")]
    UnexpectedStartOfGroup,
}
