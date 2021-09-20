use chrono::{SecondsFormat, Utc};
use plugin_sm::log_file::LogFile;
use std::path::PathBuf;

#[derive(Debug)]
pub struct OperationLogs {
    log_dir: PathBuf,
}

pub enum LogKind {
    SoftwareUpdate,
    SoftwareList,
}

impl OperationLogs {
    pub fn try_new(log_dir: PathBuf) -> Result<OperationLogs, std::io::Error> {
        std::fs::create_dir_all(log_dir.clone())?;
        Ok(OperationLogs { log_dir })
    }

    pub async fn new_log_file(&self, kind: LogKind) -> Result<LogFile, std::io::Error> {
        let now = Utc::now();
        let file_prefix = match kind {
            LogKind::SoftwareUpdate => "software-update",
            LogKind::SoftwareList => "software-list",
        };
        let file_name = format!(
            "{}-{}.log",
            file_prefix,
            now.to_rfc3339_opts(SecondsFormat::Secs, true)
        );

        let mut log_file_path = self.log_dir.clone();
        log_file_path.push(file_name);

        LogFile::try_new(log_file_path).await
    }
}
