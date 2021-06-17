use std::num::FpCategory;

#[derive(Debug, Clone)]
pub struct JsonWriter {
    buffer: Vec<u8>,
}

#[derive(thiserror::Error, Debug)]
pub enum JsonWriterError {
    #[error("JsonWriter produced invalid UTF8 string")]
    InvalidUtf8Conversion(#[from] std::string::FromUtf8Error),

    #[error("IoError")]
    IoError(#[from] std::io::Error),

    #[error("Serde Json error")]
    SerdeJsonError(#[from] serde_json::Error),

    #[error("Invalid f64 value {value:?}")]
    InvalidF64Value { value: f64 },
}

impl JsonWriter {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
        }
    }

    pub fn write_key(&mut self, key: &str) -> Result<(), JsonWriterError> {
        self.write_str(key)?;
        self.buffer.push(b':');
        Ok(())
    }

    pub fn write_str(&mut self, s: &str) -> Result<(), JsonWriterError> {
        Ok(serde_json::to_writer(&mut self.buffer, s)?)
    }

    pub fn write_f64(&mut self, value: f64) -> Result<(), JsonWriterError> {
        match value.classify() {
            FpCategory::Normal | FpCategory::Zero | FpCategory::Subnormal => {
                Ok(serde_json::to_writer(&mut self.buffer, &value)?)
            }
            FpCategory::Infinite | FpCategory::Nan => {
                Err(JsonWriterError::InvalidF64Value { value })
            }
        }
    }

    pub fn write_separator(&mut self) {
        self.buffer.push(b',');
    }

    pub fn write_open_obj(&mut self) {
        self.buffer.push(b'{');
    }

    pub fn write_close_obj(&mut self) {
        self.buffer.push(b'}');
    }

    pub fn into_string(self) -> Result<String, JsonWriterError> {
        Ok(String::from_utf8(self.buffer)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_empty_message() -> anyhow::Result<()> {
        let mut jw = JsonWriter::new();
        jw.write_open_obj();
        jw.write_close_obj();
        assert_eq!(jw.into_string()?, "{}");
        Ok(())
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
    fn write_key_with_quote() -> anyhow::Result<()> {
        let mut jw = JsonWriter::with_capacity(128);
        jw.write_key("va\"lue")?;
        assert_eq!(jw.into_string()?, "\"va\\\"lue\":");
        Ok(())
    }

    #[test]
    fn write_timestamp_message() -> anyhow::Result<()> {
        let mut jw = JsonWriter::with_capacity(128);
        jw.write_open_obj();
        jw.write_key("time")?;
        jw.write_str("2013-06-22T17:03:14.123+02:00")?;
        jw.write_close_obj();
        assert_eq!(
            jw.into_string()?,
            r#"{"time":"2013-06-22T17:03:14.123+02:00"}"#
        );
        Ok(())
    }

    #[test]
    fn write_single_value_message() -> anyhow::Result<()> {
        let mut jw = JsonWriter::with_capacity(128);
        jw.write_open_obj();
        jw.write_key("time")?;
        jw.write_str("2013-06-22T17:03:14.123+02:00")?;
        jw.write_separator();
        jw.write_key("temperature")?;
        jw.write_f64(128.0)?;
        jw.write_close_obj();
        assert_eq!(
            jw.into_string()?,
            r#"{"time":"2013-06-22T17:03:14.123+02:00","temperature":128.0}"#
        );
        Ok(())
    }

    #[test]
    fn write_multivalue_message() -> anyhow::Result<()> {
        let mut jw = JsonWriter::with_capacity(128);
        jw.write_open_obj();
        jw.write_key("time")?;
        jw.write_str("2013-06-22T17:03:14.123+02:00")?;
        jw.write_separator();
        jw.write_key("temperature")?;
        jw.write_f64(128.0)?;
        jw.write_separator();
        jw.write_key("location")?;
        jw.write_open_obj();
        jw.write_key("altitude")?;
        jw.write_f64(1028.0)?;
        jw.write_separator();
        jw.write_key("longitude")?;
        jw.write_f64(1288.0)?;
        jw.write_separator();
        jw.write_key("longitude")?;
        jw.write_f64(1280.0)?;
        jw.write_close_obj();
        jw.write_close_obj();

        assert_eq!(
            jw.into_string()?,
            r#"{"time":"2013-06-22T17:03:14.123+02:00","temperature":128.0,"location":{"altitude":1028.0,"longitude":1288.0,"longitude":1280.0}}"#
        );

        Ok(())
    }
}
