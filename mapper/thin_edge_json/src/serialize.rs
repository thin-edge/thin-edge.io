use chrono::offset::FixedOffset;
use chrono::DateTime;
use std::io::Write;

use crate::measurement::GroupedMeasurementVisitor;
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
    MeasurementCollectorError(#[from] MeasurementStreamError),
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

impl ThinEdgeJsonSerializer {
    pub fn new() -> Self {
        let mut serializer = ThinEdgeJsonSerializer {
            buffer: Vec::new(),
            is_within_group: false,
            needs_separator: false,
        };
        serializer.buffer.push(b'{');
        serializer
    }

    fn end(&mut self) -> Result<(), ThinEdgeJsonSerializationError> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedEndOfData.into());
        }

        self.buffer.push(b'}');
        Ok(())
    }

    pub fn bytes(mut self) -> Result<Vec<u8>, ThinEdgeJsonSerializationError> {
        self.end()?;
        Ok(self.buffer)
    }
}

impl Default for ThinEdgeJsonSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl GroupedMeasurementVisitor for ThinEdgeJsonSerializer {
    type Error = ThinEdgeJsonSerializationError;

    fn timestamp(&mut self, timestamp: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedTimestamp.into());
        }

        if self.needs_separator {
            self.buffer.push(b',');
        }
        self.buffer
            .write_fmt(format_args!("\"time\":\"{}\"", timestamp))?;
        self.needs_separator = true;
        Ok(())
    }

    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error> {
        if self.needs_separator {
            self.buffer.push(b',');
        }
        self.buffer
            .write_fmt(format_args!("\"{}\":{}", name, value))?;
        self.needs_separator = true;
        Ok(())
    }

    fn start_group(&mut self, group: &str) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedStartOfGroup.into());
        }

        if self.needs_separator {
            self.buffer.push(b',');
        }
        self.buffer.write_fmt(format_args!("\"{}\":{{", group))?;
        self.needs_separator = false;
        self.is_within_group = true;
        Ok(())
    }

    fn end_group(&mut self) -> Result<(), Self::Error> {
        if !self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedEndOfGroup.into());
        }

        self.buffer.push(b'}');
        self.needs_separator = true;
        self.is_within_group = false;
        Ok(())
    }
}
