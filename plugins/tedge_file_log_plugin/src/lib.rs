pub mod bin;

mod config;
mod error;
mod log_utils;

pub use config::*;
pub use error::*;
pub use log_utils::*;

use camino::Utf8Path;
use camino::Utf8PathBuf;
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
        since: Option<OffsetDateTime>,
        _until: Option<OffsetDateTime>,
    ) -> Result<Utf8PathBuf, LogManagementError> {
        let date_from = since.unwrap_or(OffsetDateTime::UNIX_EPOCH);

        let log_path = new_read_logs(&self.config.files, log_type, date_from, &self.tmp_dir)?;

        Ok(log_path)
    }
}
