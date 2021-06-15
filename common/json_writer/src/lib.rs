use std::fmt::Write;

#[derive(Debug, Clone)]
pub struct JsonWriter {
    buffer: String,
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

    pub fn write_key_noescape(&mut self, key: &str) {
        self.write_str_noescape(key);
        self.buffer.push(':');
    }

    pub fn write_str_noescape(&mut self, s: &str) {
        self.buffer.push('"');
        self.buffer.push_str(s);
        self.buffer.push('"');
    }

    pub fn write_f64(&mut self, value: f64) -> Result<(), std::fmt::Error> {
        Ok(self.buffer.write_fmt(format_args!("{}", value))?)
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

    #[test]
    fn write_empty_message() {
        let mut jw = JsonWriter::new();
        jw.write_open_obj();
        jw.write_close_obj();
        assert_eq!(jw.into_string(), "{}");
    }

    #[test]
    fn write_timestamp_message() {
        let mut jw = JsonWriter::with_capacity(128);
        jw.write_open_obj();
        jw.write_key_noescape("time");
        jw.write_str_noescape("2013-06-22T17:03:14.123+02:00");
        jw.write_close_obj();
        assert_eq!(
            jw.into_string(),
            r#"{"time":"2013-06-22T17:03:14.123+02:00"}"#
        );
    }

    #[test]
    fn write_single_value_message() -> anyhow::Result<()> {
        let mut jw = JsonWriter::with_capacity(128);
        jw.write_open_obj();
        jw.write_key_noescape("time");
        jw.write_str_noescape("2013-06-22T17:03:14.123+02:00");
        jw.write_separator();
        jw.write_key_noescape("temperature");
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
        jw.write_key_noescape("time");
        jw.write_str_noescape("2013-06-22T17:03:14.123+02:00");
        jw.write_separator();
        jw.write_key_noescape("temperature");
        jw.write_f64(128.0)?;
        jw.write_separator();
        jw.write_key_noescape("location");
        jw.write_open_obj();
        jw.write_key_noescape("altitude");
        jw.write_f64(1028.0)?;
        jw.write_separator();
        jw.write_key_noescape("longitude");
        jw.write_f64(1288.0)?;
        jw.write_separator();
        jw.write_key_noescape("longitude");
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
