use chrono::{DateTime, FixedOffset};

pub trait MeasurementVisitor {
    type Error;

    fn visit_measurement_type(&mut self, typename: &str) -> Result<(), Self::Error>;
    fn visit_timestamp(&mut self, timestamp: DateTime<FixedOffset>) -> Result<(), Self::Error>;

    fn visit_measurement_data(&mut self, key: &str, value: f64) -> Result<(), Self::Error>;

    fn visit_start_measurement_group(&mut self, key: &str) -> Result<(), Self::Error>;
    fn visit_end_measurement_group(&mut self) -> Result<(), Self::Error>;

    fn visit_start(&mut self) -> Result<(), Self::Error>;
    fn visit_end(&mut self) -> Result<(), Self::Error>;
}
