use std::path::PathBuf;
use tokio::fs::File;
use chrono::{Utc, SecondsFormat};

#[derive(Debug)]
pub struct OperationLogs {
    pub log_dir: PathBuf,
}

impl OperationLogs {
    pub fn try_new(log_dir: PathBuf) -> Result<OperationLogs, std::io::Error> {
        std::fs::create_dir_all(log_dir.clone())?;
        Ok(OperationLogs { log_dir })
    }

    pub async fn new_log_file(&self) -> Result<File, std::io::Error> {
        let now = Utc::now();
        let file_name = format!("software-update-{}.log", now.to_rfc3339_opts(SecondsFormat::Secs, true));

        let mut log_file_path = self.log_dir.clone();
        log_file_path.push(file_name);

        let log_file = File::create(log_file_path).await?;

        Ok(log_file)
    }
}
