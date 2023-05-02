use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use time::format_description;
use time::OffsetDateTime;
use tracing::log;

use crate::log_file::LogFile;

#[derive(Debug, thiserror::Error)]
pub enum OperationLogsError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromTimeFormat(#[from] time::error::Format),

    #[error("Incorrect file format. Expected: `operation_name`-`timestamp`.log")]
    FileFormatError,
}

#[derive(Debug)]
pub struct OperationLogs {
    pub log_dir: PathBuf,
}

pub enum LogKind {
    SoftwareUpdate,
    SoftwareList,
    Operation(String),
}

const UPDATE_PREFIX: &str = "software-update";
const LIST_PREFIX: &str = "software-list";

impl OperationLogs {
    pub fn try_new(log_dir: PathBuf) -> Result<OperationLogs, OperationLogsError> {
        std::fs::create_dir_all(log_dir.clone())?;
        let operation_logs = OperationLogs { log_dir };

        if let Err(err) = operation_logs.remove_outdated_logs() {
            // In no case a log-cleaning error should prevent the agent to run.
            // Hence the error is logged but not returned.
            log::warn!("Fail to remove the out-dated log files: {}", err);
        }

        Ok(operation_logs)
    }

    pub async fn new_log_file(&self, kind: LogKind) -> Result<LogFile, OperationLogsError> {
        if let Err(err) = self.remove_outdated_logs() {
            // In no case a log-cleaning error should prevent the agent to run.
            // Hence the error is logged but not returned.
            log::warn!("Fail to remove the out-dated log files: {}", err);
        }

        let now = OffsetDateTime::now_utc();

        let file_prefix = match kind {
            LogKind::SoftwareUpdate => UPDATE_PREFIX,
            LogKind::SoftwareList => LIST_PREFIX,
            LogKind::Operation(ref operation_name) => operation_name.as_str(),
        };

        let file_name = format!(
            "{}-{}.log",
            file_prefix,
            now.format(&format_description::well_known::Rfc3339)?
        );

        let mut log_file_path = self.log_dir.clone();
        log_file_path.push(file_name);

        LogFile::try_new(log_file_path)
            .await
            .map_err(OperationLogsError::FromIo)
    }

    pub fn remove_outdated_logs(&self) -> Result<(), OperationLogsError> {
        let mut log_tracker: HashMap<String, BinaryHeap<Reverse<String>>> = HashMap::new();

        // FIXME: this is a hotfix to map "software-list" and "software-update" to "software-management"
        // this should be fixed in https://github.com/thin-edge/thin-edge.io/issues/1077
        for file in (self.log_dir.read_dir()?).flatten() {
            if let Some(path) = file.path().file_name().and_then(|name| name.to_str()) {
                if path.starts_with("software-list") {
                    log_tracker
                        .entry("software-list".to_string())
                        .or_insert_with(BinaryHeap::new)
                        .push(Reverse(path.to_string()));
                } else if path.starts_with("software-update") {
                    log_tracker
                        .entry("software-update".to_string())
                        .or_insert_with(BinaryHeap::new)
                        .push(Reverse(path.to_string()));
                } else {
                    let file_name = path
                        .split('-')
                        .next()
                        .ok_or(OperationLogsError::FileFormatError)?;
                    log_tracker
                        .entry(file_name.to_string())
                        .or_insert_with(BinaryHeap::new)
                        .push(Reverse(path.to_string()));
                }
            }
        }

        for (key, value) in log_tracker.iter_mut() {
            if key.starts_with("software-list") {
                // only allow one update list file in logs
                remove_old_logs(value, &self.log_dir, 1)?;
            } else {
                // allow most recent five
                remove_old_logs(value, &self.log_dir, 5)?;
            }
        }

        Ok(())
    }
}

fn remove_old_logs(
    log_tracker: &mut BinaryHeap<Reverse<String>>,
    dir_path: &Path,
    n: usize,
) -> Result<(), OperationLogsError> {
    while log_tracker.len() > n {
        if let Some(rname) = log_tracker.pop() {
            let name = rname.0;
            let path = dir_path.join(name.clone());
            if let Err(err) = std::fs::remove_file(path) {
                log::warn!("Fail to remove out-dated log file {} : {}", name, err);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::path::Path;
    use tempfile::TempDir;

    #[tokio::test]
    async fn on_start_keeps_only_the_latest_logs() -> Result<(), anyhow::Error> {
        // Create a log dir with a bunch of fake log files
        let log_dir = TempDir::new()?;

        let swlist_log_1 = create_file(log_dir.path(), "software-list-1996-02-22T16:39:57z");
        let update_log_1 = create_file(log_dir.path(), "software-update-1996-12-19T16:39:57z");
        let update_log_2 = create_file(log_dir.path(), "software-update-1996-12-20T16:39:57z");
        let update_log_3 = create_file(log_dir.path(), "software-update-1996-12-21T16:39:57z");
        let update_log_4 = create_file(log_dir.path(), "software-update-1996-12-22T16:39:57z");
        let swlist_log_2 = create_file(log_dir.path(), "software-list-1996-12-22T16:39:57z");
        let update_log_5 = create_file(log_dir.path(), "software-update-1996-12-23T16:39:57z");
        let update_log_6 = create_file(log_dir.path(), "software-update-1996-12-24T16:39:57z");
        let update_log_7 = create_file(log_dir.path(), "software-update-1996-12-25T16:39:57z");
        let unrelated_1 = create_file(log_dir.path(), "foo");
        let unrelated_2 = create_file(log_dir.path(), "bar");

        // Open the log dir
        let _operation_logs = OperationLogs::try_new(log_dir.path().to_path_buf())?;

        // Outdated logs are removed
        assert!(!update_log_1.exists());
        assert!(!update_log_2.exists());
        assert!(!swlist_log_1.exists());

        // The 5 latest update logs are kept
        assert!(update_log_3.exists());
        assert!(update_log_4.exists());
        assert!(update_log_5.exists());
        assert!(update_log_6.exists());
        assert!(update_log_7.exists());

        // The latest software list is kept
        assert!(swlist_log_2.exists());

        // Unrelated files are untouched
        assert!(unrelated_1.exists());
        assert!(unrelated_2.exists());

        Ok(())
    }

    #[tokio::test]
    async fn on_new_log_keep_the_latest_logs_plus_the_new_one() -> Result<(), anyhow::Error> {
        // Create a log dir
        let log_dir = TempDir::new()?;
        let operation_logs = OperationLogs::try_new(log_dir.path().to_path_buf())?;

        // Add a bunch of fake log files
        let swlist_log_1 = create_file(log_dir.path(), "software-list-1996-02-22T16:39:57z");
        let update_log_1 = create_file(log_dir.path(), "software-update-1996-12-19T16:39:57z");
        let update_log_2 = create_file(log_dir.path(), "software-update-1996-12-20T16:39:57z");
        let update_log_3 = create_file(log_dir.path(), "software-update-1996-12-21T16:39:57z");
        let update_log_4 = create_file(log_dir.path(), "software-update-1996-12-22T16:39:57z");
        let swlist_log_2 = create_file(log_dir.path(), "software-list-1996-12-22T16:39:57z");
        let update_log_5 = create_file(log_dir.path(), "software-update-1996-12-23T16:39:57z");
        let update_log_6 = create_file(log_dir.path(), "software-update-1996-12-24T16:39:57z");
        let update_log_7 = create_file(log_dir.path(), "software-update-1996-12-25T16:39:57z");

        // Create a new log file
        let new_log = operation_logs.new_log_file(LogKind::SoftwareUpdate).await?;

        // The new log has been created
        let new_path = Path::new(new_log.path());
        assert!(new_path.exists());

        // Outdated logs are removed
        assert!(!update_log_1.exists());
        assert!(!update_log_2.exists());
        assert!(!swlist_log_1.exists());

        // The 5 latest update logs are kept
        assert!(update_log_3.exists());
        assert!(update_log_4.exists());
        assert!(update_log_5.exists());
        assert!(update_log_6.exists());
        assert!(update_log_7.exists());

        // The latest software list is kept
        assert!(swlist_log_2.exists());

        Ok(())
    }

    fn create_file(dir: &Path, name: &str) -> PathBuf {
        let file_path = dir.join(name);
        let _log_file = File::create(file_path.clone()).expect("fail to create a test file");
        file_path
    }
}
