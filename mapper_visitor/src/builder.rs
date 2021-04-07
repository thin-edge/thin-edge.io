use crate::*;
use chrono::{DateTime, FixedOffset};

pub struct MeasurementBuilder<'a, T: MeasurementVisitor> {
    visitor: &'a mut T,
}

impl<'a, T> MeasurementBuilder<'a, T>
where
    T: MeasurementVisitor,
{
    pub fn new(visitor: &'a mut T) -> Self {
        Self { visitor }
    }

    pub fn measurement_type(self, typename: impl AsRef<str>) -> Result<Self, T::Error> {
        let () = self.visitor.visit_measurement_type(typename.as_ref())?;
        Ok(self)
    }

    pub fn timestamp(self, timestamp: DateTime<FixedOffset>) -> Result<Self, T::Error> {
        let () = self.visitor.visit_timestamp(timestamp)?;
        Ok(self)
    }

    pub fn measurement_data(
        self,
        key: impl AsRef<str>,
        value: impl Into<f64>,
    ) -> Result<Self, T::Error> {
        let () = self
            .visitor
            .visit_measurement_data(key.as_ref(), value.into())?;
        Ok(self)
    }

    pub fn start_group(self, key: impl AsRef<str>) -> Result<Self, T::Error> {
        let () = self.visitor.visit_start_measurement_group(key.as_ref())?;
        Ok(self)
    }

    pub fn end_group(self) -> Result<Self, T::Error> {
        let () = self.visitor.visit_end_measurement_group()?;
        Ok(self)
    }

    pub fn start(self) -> Result<Self, T::Error> {
        let () = self.visitor.visit_start()?;
        Ok(self)
    }

    pub fn end(self) -> Result<(), T::Error> {
        let () = self.visitor.visit_end()?;
        Ok(())
    }
}
