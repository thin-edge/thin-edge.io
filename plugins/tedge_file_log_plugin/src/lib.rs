pub mod bin;

mod config;
mod error;
mod log_utils;

pub use config::*;
pub use error::*;
pub use log_utils::*;

use camino::Utf8Path;
use std::sync::Arc;
use tedge_api::CommandLog;
use time::OffsetDateTime;

#[derive(Debug)]
pub struct FileLogPlugin {
    config: LogPluginConfig,
    tmp_dir: Arc<Utf8Path>,
}

impl FileLogPlugin {
    pub fn new(config: LogPluginConfig, tmp_dir: Arc<Utf8Path>) -> Self {
        Self { config, tmp_dir }
    }

    fn list(
        &self,
        _command_log: Option<&mut CommandLog>,
    ) -> Result<Vec<String>, LogManagementError> {
        Ok(self.config.get_all_file_types())
    }

    fn get(
        &self,
        log_type: &str,
        output_file_path: &Utf8Path,
        since: Option<OffsetDateTime>,
        _until: Option<OffsetDateTime>,
        filter_text: Option<&str>,
        lines: Option<usize>,
    ) -> Result<(), LogManagementError> {
        // Use the existing file-based log retrieval logic
        let date_from = since.unwrap_or(OffsetDateTime::UNIX_EPOCH);
        let lines = lines.unwrap_or(1000);
        let search_text = filter_text.map(|s| s.to_string());

        let log_path = new_read_logs(
            &self.config.files,
            log_type,
            date_from,
            lines,
            &search_text,
            &self.tmp_dir,
        )?;

        // Copy the generated log file to the requested temp file path
        std::fs::copy(&log_path, output_file_path)?;

        // Clean up the temporary file
        if let Err(e) = std::fs::remove_file(&log_path) {
            log::warn!("Failed to remove temporary file {}: {}", log_path, e);
        }

        Ok(())
    }
}
