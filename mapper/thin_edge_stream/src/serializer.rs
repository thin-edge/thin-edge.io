use std::io::Write;
use chrono::{DateTime, FixedOffset};
use crate::builder::GroupedMeasurementCollector;
use crate::builder::MeasurementCollectorError;

/// Serialize a series of measurements into a ThinEdgeJson byte-string.
///
/// Perform no check beyond the fact that groups are properly closed.
pub struct ThinEdgeJsonSerializer {
    buffer: Vec<u8>,
    is_within_group: bool,
    needs_separator: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeJsonSerializationError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    MeasurementCollectorError(#[from] MeasurementCollectorError),
}

impl ThinEdgeJsonSerializer {
    pub fn new() -> ThinEdgeJsonSerializer {
        ThinEdgeJsonSerializer {
            buffer: Vec::new(),
            is_within_group: false,
            needs_separator: false,
        }
    }
}

impl GroupedMeasurementCollector for ThinEdgeJsonSerializer {
    type Error = ThinEdgeJsonSerializationError;
    type Data = Vec<u8>;

    fn start(&mut self) -> Result<(), Self::Error> {
        self.buffer.push(b'{');
        self.needs_separator = false;
        Ok(())
    }

    fn end(mut self) -> Result<Self::Data, Self::Error> {
        if self.is_within_group {
            return Err(MeasurementCollectorError::UnexpectedEndOfData.into());
        }

        self.buffer.push(b'}');
        Ok(self.buffer)
    }

    fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementCollectorError::UnexpectedTimestamp.into());
        }

        if self.needs_separator {
            self.buffer.push(b',');
        }
        self.buffer.write_fmt(format_args!("\"time\":\"{}\"", value.to_rfc3339()))?;
        self.needs_separator = true;
        Ok(())
    }

    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error> {
        if self.needs_separator {
            self.buffer.push(b',');
        }
        self.buffer.write_fmt(format_args!("\"{}\":{}", name, value))?;
        self.needs_separator = true;
        Ok(())
    }

    fn start_group(&mut self, group: &str) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementCollectorError::UnexpectedStartOfGroup.into());
        }

        if self.needs_separator {
            self.buffer.push(b',');
        }
        self.buffer.write_fmt(format_args!("\"{}\":{{", group))?;
        self.needs_separator = false;
        Ok(())
    }

    fn end_group(&mut self) -> Result<(), Self::Error> {
        if ! self.is_within_group {
            return Err(MeasurementCollectorError::UnexpectedEndOfGroup.into());
        }

        self.buffer.push(b'}');
        self.needs_separator = true;
        Ok(())
    }
}
