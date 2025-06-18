use crate::error;
use crate::info;
use crate::warning;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// Log to both stderr and a file
#[derive(Debug)]
pub struct DualLogger {
    file: File,
}

impl DualLogger {
    pub fn new(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(DualLogger { file })
    }

    pub fn log(&mut self, message: &str) {
        eprintln!("{message}");
        let _ = writeln!(self.file, "{message}");
    }

    pub fn info(&mut self, message: &str) {
        info!("{message}");
        let _ = writeln!(self.file, "info: {message}");
    }

    pub fn warning(&mut self, message: &str) {
        warning!("{message}");
        let _ = writeln!(self.file, "warning: {message}");
    }

    pub fn error(&mut self, message: &str) {
        error!("{message}");
        let _ = writeln!(self.file, "error: {message}");
    }

    pub fn only_to_file(&mut self, message: &str) {
        let _ = writeln!(self.file, "{message}");
    }
}
