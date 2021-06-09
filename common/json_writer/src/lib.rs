use std::fmt::Write;

pub struct JsonWriter {
    buffer: String,
}

impl JsonWriter {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }
    pub fn with_capacity(capa : usize) -> Self {
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

    pub fn write_raw(&mut self, s: &str) {
        self.buffer.push_str(s);
    }

    pub fn write_f64(&mut self, value: f64) -> std::fmt::Result {
        self.buffer.write_fmt(format_args!("{}", value))
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
