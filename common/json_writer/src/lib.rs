use std::fmt::Write;
use std::num::FpCategory;

#[derive(Debug, Clone)]
pub struct JsonWriter {
    buffer: String,
}
#[derive(thiserror::Error, Debug, PartialEq)]
pub enum JsonWriterError {
    #[error(transparent)]
    Writef64ValueError(#[from] std::fmt::Error),

    #[error("Invalid f64 value {value:?}")]
    InvalidF64Value { value: f64 },

    #[error("Invalid safe str value {value:?}")]
    InvaidSafeStr { value: String },
}

/// String slice that does not need escaping
#[derive(Debug, Clone)]
pub struct SafeStr<'a>(&'a str);

impl<'a> SafeStr<'a> {
    pub fn as_str(&self) -> &str {
        self.0
    }
}

impl<'a> std::convert::TryFrom<&'a str> for SafeStr<'a> {
    type Error = JsonWriterError;
    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        if s.contains('\\') {
            Err(JsonWriterError::InvaidSafeStr { value: s.into() })
        } else {
            Ok(Self(s))
        }
    }
}

impl JsonWriter {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }
    pub fn with_capacity(capa: usize) -> Self {
        Self {
            buffer: String::with_capacity(capa),
        }
    }

    pub fn write_static_key(&mut self, key: &str) {
        self.write_static_str_noescape(key);
        self.buffer.push(':');
    }

    pub fn write_static_str_noescape(&mut self, s: &str) {
        self.buffer.push('"');
        self.buffer.push_str(s);
        self.buffer.push('"');
    }

    pub fn write_key_noescape(&mut self, key: SafeStr) {
        self.write_str_noescape(key);
        self.buffer.push(':');
    }

    pub fn write_str_noescape(&mut self, s: SafeStr) {
        self.buffer.push('"');
        self.buffer.push_str(s.as_str());
        self.buffer.push('"');
    }

    pub fn write_f64(&mut self, value: f64) -> Result<(), JsonWriterError> {
        match value.classify() {
            FpCategory::Normal | FpCategory::Zero | FpCategory::Subnormal => {
                Ok(self.buffer.write_fmt(format_args!("{}", value))?)
            }
            FpCategory::Infinite | FpCategory::Nan => {
                Err(JsonWriterError::InvalidF64Value { value })
            }
        }
    }

    pub fn write_separator(&mut self) {
        self.buffer.push(',');
    }

    pub fn write_open_obj(&mut self) {
        self.buffer.push('{');
    }

    pub fn write_close_obj(&mut self) {
        self.buffer.push('}');
    }

    pub fn into_string(self) -> String {
        self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::*;

    #[test]
    fn write_empty_message() {
        let mut jw = JsonWriter::new();
        jw.write_open_obj();
        jw.write_close_obj();
        assert_eq!(jw.into_string(), "{}");
    }

    #[test]
    fn write_invalid_f64_message() -> anyhow::Result<()> {
        let mut jw = JsonWriter::new();
        let value = 1.0 / 0.0;
        let error = jw.write_f64(value).unwrap_err();
        assert_eq!(error.to_string(), "Invalid f64 value inf");
        Ok(())
    }

    #[test]
    fn write_invalid_safestr() -> anyhow::Result<()> {
        let err = SafeStr::try_from("Not\\safe").unwrap_err();
        assert_eq!(err.to_string(), "Invalid safe str value \"Not\\\\safe\"");
        Ok(())
    }

    #[test]
    fn write_timestamp_message() -> anyhow::Result<()> {
        let mut jw = JsonWriter::with_capacity(128);
        jw.write_open_obj();
        jw.write_key_noescape("time".try_into()?);
        jw.write_str_noescape("2013-06-22T17:03:14.123+02:00".try_into()?);
        jw.write_close_obj();
        assert_eq!(
            jw.into_string(),
            r#"{"time":"2013-06-22T17:03:14.123+02:00"}"#
        );
        Ok(())
    }

    #[test]
    fn write_single_value_message() -> anyhow::Result<()> {
        let mut jw = JsonWriter::with_capacity(128);
        jw.write_open_obj();
        jw.write_key_noescape("time".try_into()?);
        jw.write_str_noescape("2013-06-22T17:03:14.123+02:00".try_into()?);
        jw.write_separator();
        jw.write_key_noescape("temperature".try_into()?);
        jw.write_f64(128.0)?;
        jw.write_close_obj();
        assert_eq!(
            jw.into_string(),
            r#"{"time":"2013-06-22T17:03:14.123+02:00","temperature":128}"#
        );
        Ok(())
    }

    #[test]
    fn write_multivalue_message() -> anyhow::Result<()> {
        let mut jw = JsonWriter::with_capacity(128);
        jw.write_open_obj();
        jw.write_key_noescape("time".try_into()?);
        jw.write_str_noescape("2013-06-22T17:03:14.123+02:00".try_into()?);
        jw.write_separator();
        jw.write_key_noescape("temperature".try_into()?);
        jw.write_f64(128.0)?;
        jw.write_separator();
        jw.write_key_noescape("location".try_into()?);
        jw.write_open_obj();
        jw.write_key_noescape("altitude".try_into()?);
        jw.write_f64(1028.0)?;
        jw.write_separator();
        jw.write_key_noescape("longitude".try_into()?);
        jw.write_f64(1288.0)?;
        jw.write_separator();
        jw.write_key_noescape("longitude".try_into()?);
        jw.write_f64(1280.0)?;
        jw.write_close_obj();
        jw.write_close_obj();

        assert_eq!(
            jw.into_string(),
            r#"{"time":"2013-06-22T17:03:14.123+02:00","temperature":128,"location":{"altitude":1028,"longitude":1288,"longitude":1280}}"#
        );

        Ok(())
    }
}
