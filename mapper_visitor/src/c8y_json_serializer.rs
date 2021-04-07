use crate::*;
use chrono::{DateTime, FixedOffset};
use std::io::Write;

pub struct C8yJsonSerializer {
    buffer: Vec<u8>,
    default_typename: String,
    default_timestamp: DateTime<FixedOffset>,
    is_complete: bool,
    is_within_group: bool,
    needs_separator: bool,
    has_seen_type: bool,
    has_seen_timestamp: bool,
}

impl C8yJsonSerializer {
    pub fn new(default_typename: String, default_timestamp: DateTime<FixedOffset>) -> Self {
        Self {
            buffer: Vec::new(),
            default_typename,
            default_timestamp,
            is_complete: false,
            is_within_group: false,
            needs_separator: false,
            has_seen_type: false,
            has_seen_timestamp: false,
        }
    }

    pub fn data(self) -> Vec<u8> {
        assert!(self.is_complete);
        self.buffer
    }
}

impl MeasurementVisitor for C8yJsonSerializer {
    type Error = MeasurementError;

    fn visit_measurement_type(&mut self, typename: &str) -> Result<(), Self::Error> {
        assert!(!self.is_within_group);
        assert!(!self.has_seen_type);

        // XXX: write type
        self.has_seen_type = true;

        Ok(())
    }

    fn visit_timestamp(&mut self, timestamp: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        assert!(!self.is_within_group);
        assert!(!self.has_seen_timestamp);

        // XXX: write timestamp
        self.has_seen_timestamp = true;

        Ok(())
    }

    fn visit_measurement_data(&mut self, key: &str, value: f64) -> Result<(), Self::Error> {
        if self.needs_separator {
            self.buffer.push(b',');
        } else {
            self.needs_separator = true;
        }
        if self.is_within_group {
            self.buffer
                .write_fmt(format_args!(r#""{}": {{"value": {}}}"#, key, value))
                .unwrap();
        } else {
            self.buffer
                .write_fmt(format_args!(
                    r#""{}": {{"{}": {{"value": {}}}}}"#,
                    key, key, value
                ))
                .unwrap();
        }
        Ok(())
    }

    fn visit_start_measurement_group(&mut self, key: &str) -> Result<(), Self::Error> {
        assert!(!self.is_within_group, "Nested groups not supported"); // XXX: Error
        self.is_within_group = true;

        if self.needs_separator {
            self.buffer.push(b',');
        }

        self.buffer
            .write_fmt(format_args!(r#""{}": {{"#, key))
            .unwrap();
        self.needs_separator = false;
        Ok(())
    }

    fn visit_end_measurement_group(&mut self) -> Result<(), Self::Error> {
        assert!(self.is_within_group);

        self.buffer.push(b'}');

        self.is_within_group = false;
        Ok(())
    }

    fn visit_start(&mut self) -> Result<(), Self::Error> {
        self.buffer.push(b'{');
        Ok(())
    }

    fn visit_end(&mut self) -> Result<(), Self::Error> {
        assert!(!self.is_within_group);

        if !self.has_seen_type {
            // XXX: write default_typename
        }
        if !self.has_seen_timestamp {
            // XXX: write default_timestamp
        }

        self.buffer.push(b'}');
        self.is_complete = true;
        Ok(())
    }
}
