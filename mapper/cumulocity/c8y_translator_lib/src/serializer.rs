use chrono::prelude::*;
use std::io::Write;
use thin_edge_json::{json::ThinEdgeJsonError, measurement::GroupedMeasurementVisitor};

pub struct C8yJsonSerializer {
    buffer: Vec<u8>,
    is_within_group: bool,
    needs_separator: bool,
    timestamp_present: bool,
    default_timestamp: DateTime<FixedOffset>,
}

#[derive(thiserror::Error, Debug)]
pub enum C8yJsonSerializationError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    MeasurementCollectorError(#[from] MeasurementStreamError),

    #[error(transparent)]
    ThinEdgeJsonParseError(#[from] ThinEdgeJsonError),
}
#[derive(thiserror::Error, Debug, Eq, PartialEq)]
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

impl C8yJsonSerializer {
    pub fn new(
        default_timestamp: DateTime<FixedOffset>,
    ) -> Result<Self, C8yJsonSerializationError> {
        let mut serializer = C8yJsonSerializer {
            buffer: Vec::new(),
            is_within_group: false,
            needs_separator: true,
            timestamp_present: false,
            default_timestamp,
        };

        let _ = serializer
            .buffer
            .write(b"{\"type\": \"ThinEdgeMeasurement\"")?;
        Ok(serializer)
    }

    fn end(&mut self) -> Result<(), C8yJsonSerializationError> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedEndOfData.into());
        }

        if !self.timestamp_present {
            self.timestamp(self.default_timestamp)?;
        }

        assert!(self.timestamp_present);

        self.buffer.push(b'}');
        Ok(())
    }

    pub fn bytes(mut self) -> Result<Vec<u8>, C8yJsonSerializationError> {
        self.end()?;
        Ok(self.buffer)
    }
}

impl GroupedMeasurementVisitor for C8yJsonSerializer {
    type Error = C8yJsonSerializationError;

    fn timestamp(&mut self, timestamp: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedTimestamp.into());
        }

        if self.needs_separator {
            self.buffer.push(b',');
        }
        self.buffer
            .write_fmt(format_args!("\"time\":\"{}\"", timestamp.to_rfc3339()))?;
        self.needs_separator = true;
        self.timestamp_present = true;
        Ok(())
    }

    fn measurement(&mut self, key: &str, value: f64) -> Result<(), Self::Error> {
        if self.needs_separator {
            self.buffer.push(b',');
        } else {
            self.needs_separator = true;
        }
        if self.is_within_group {
            self.buffer
                .write_fmt(format_args!(r#""{}": {{"value": {}}}"#, key, value))?;
        } else {
            self.buffer.write_fmt(format_args!(
                r#""{}": {{"{}": {{"value": {}}}}}"#,
                key, key, value
            ))?;
        }
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
