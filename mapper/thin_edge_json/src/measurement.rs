use chrono::offset::FixedOffset;
use chrono::DateTime;
pub trait FlatMeasurementVisitor {
    type Error;

    fn timestamp(&mut self, value: &DateTime<FixedOffset>) -> Result<(), Self::Error>;
    fn measurement(
        &mut self,
        group: Option<&str>,
        name: &str,
        value: f64,
    ) -> Result<(), Self::Error>;
}

pub trait GroupedMeasurementVisitor {
    type Error;

    fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error>;
    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error>;
    fn start_group(&mut self, group: &str) -> Result<(), Self::Error>;
    fn end_group(&mut self) -> Result<(), Self::Error>;
}
