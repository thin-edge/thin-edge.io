use crate::workflow::CommandId;
use crate::workflow::OperationName;
use crate::CommandLog;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;
use std::vec;
use tedge_utils::paths::ManagedDir;
use time::format_description;
use time::OffsetDateTime;
use tracing::info;
use tracing::warn;

#[derive(Debug, thiserror::Error)]
pub enum OperationLogsError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromTimeFormat(#[from] time::error::Format),
}

#[derive(Clone, Debug)]
pub struct OperationLogs {
    pub log_dir: ManagedDir,
    keep_default: Option<u32>,
    keep_per_operation: HashMap<String, u32>,
}

impl OperationLogs {
    pub fn new(log_dir: ManagedDir) -> OperationLogs {
        OperationLogs {
            log_dir,
            keep_default: Some(5),
            keep_per_operation: HashMap::new(),
        }
    }

    pub fn keep_default(&mut self, count: Option<u32>) {
        self.keep_default = count;
    }

    pub fn keep_count(&mut self, operation: &str, count: u32) {
        self.keep_per_operation.insert(operation.to_string(), count);
    }

    pub async fn new_command_log(
        &self,
        operation: String,
        cmd_id: String,
        invoking_operations: Vec<OperationName>,
        root_operation: Option<OperationName>,
        root_cmd_id: Option<CommandId>,
    ) -> CommandLog {
        let mut command_log = CommandLog::new(
            self.log_dir.path(),
            operation,
            cmd_id,
            invoking_operations,
            root_operation,
            root_cmd_id,
        );

        self.ensure_file_exists(&mut command_log).await;
        command_log
    }

    pub async fn new_log_file(
        &self,
        operation_name: String,
        cmd_id: String,
    ) -> Result<CommandLog, OperationLogsError> {
        let now = OffsetDateTime::now_utc();
        let file_name = format!(
            "{}-{}.log",
            operation_name,
            now.format(&format_description::well_known::Rfc3339)?
        );
        let file_path = self.log_dir.path().join(file_name);

        let mut command_log = CommandLog::from_log_path(file_path, operation_name, cmd_id);
        self.ensure_file_exists(&mut command_log).await;
        Ok(command_log)
    }

    pub async fn remove_all_outdated_logs(&self) {
        for (operation, keep) in self.keep_per_operation.iter() {
            if let Err(err) = self.remove_outdated_logs(operation, *keep).await {
                warn!("Fail to remove out-dated log files: {}", err);
            }
        }
    }

    /// Ensure the log file exists
    ///
    /// possibly cleaning first old logs for the same operation type
    async fn ensure_file_exists(&self, log: &mut CommandLog) {
        if tokio::fs::try_exists(&log.path).await.unwrap_or(false) {
            return;
        }

        if let Some(keep_at_most) = self
            .keep_per_operation
            .get(&log.operation)
            .or(self.keep_default.as_ref())
            .copied()
        {
            let keep = if keep_at_most > 0 {
                // Make room for the new log
                keep_at_most - 1
            } else {
                0
            };
            if let Err(err) = self.remove_outdated_logs(&log.operation, keep).await {
                // In no case a log-cleaning error should prevent the agent to run.
                // Hence the error is logged but not returned.
                warn!("Fail to remove out-dated log files: {}", err);
            }
        }

        let _ = log.open().await;
    }

    async fn remove_outdated_logs(
        &self,
        operation: &str,
        keep: u32,
    ) -> Result<(), OperationLogsError> {
        // Collect logs for that operation
        let mut operation_logs: Vec<(PathBuf, SystemTime)> = vec![];
        for file in (self.log_dir.path().read_dir()?).flatten() {
            if file
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| name.contains(operation))
                .is_some()
            {
                if let Ok(creation_date) = tokio::fs::metadata(file.path())
                    .await
                    .and_then(|metadata| metadata.created().or_else(|_| metadata.modified()))
                {
                    operation_logs.push((file.path(), creation_date))
                }
            }
        }

        // Sort the logs by ascending creation date
        operation_logs.sort_by_key(|(_, creation_date)| *creation_date);

        // Remove the 5 most recent files
        for _ in 0..keep {
            operation_logs.pop();
        }

        // Delete the others
        for (path, _) in operation_logs {
            if let Err(err) = tokio::fs::remove_file(&path).await {
                warn!(
                    "Fail to remove out-dated log file {} : {}",
                    path.display(),
                    err
                );
            } else {
                info!("Removed out-dated log file: {}", path.display());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8Path;
    use std::fs::File;
    use std::path::Path;
    use std::path::PathBuf;
    use std::time::Duration;
    use tedge_utils::paths::TedgePaths;
    use tempfile::TempDir;

    fn managed_dir(path: &Path) -> ManagedDir {
        let path = Utf8Path::from_path(path).unwrap();
        TedgePaths::from_root_with_defaults(path, "", "").root_dir()
    }

    #[tokio::test]
    async fn on_start_keeps_only_the_latest_logs() -> Result<(), anyhow::Error> {
        // Create a log dir with a bunch of fake log files
        let log_dir = TempDir::new()?;

        let swlist_log_1 = create_file(log_dir.path(), "software-list-cmd001").await;
        let update_log_1 = create_file(log_dir.path(), "software-update-cmd002").await;
        let update_log_2 = create_file(log_dir.path(), "software-update-cmd003").await;
        let update_log_3 = create_file(log_dir.path(), "software-update-cmd004").await;
        let update_log_4 = create_file(log_dir.path(), "software-update-cmd005").await;
        let swlist_log_2 = create_file(log_dir.path(), "software-list-cmd006").await;
        let update_log_5 = create_file(log_dir.path(), "software-update-1996-cmd007").await;
        let update_log_6 = create_file(log_dir.path(), "software-update-1996-cmd008").await;
        let update_log_7 = create_file(log_dir.path(), "software-update-1996-cmd009").await;
        let unrelated_1 = create_file(log_dir.path(), "foo").await;
        let unrelated_2 = create_file(log_dir.path(), "bar").await;

        // Open the log dir
        let mut operation_logs = OperationLogs::new(managed_dir(log_dir.path()));
        operation_logs.keep_count("software-list", 1);
        operation_logs.keep_count("software-update", 5);
        operation_logs.remove_all_outdated_logs().await;

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
        let operation_logs = OperationLogs::new(managed_dir(log_dir.path()));

        // Add a bunch of fake log files
        let swlist_log_1 = create_file(log_dir.path(), "software-list-cmd001").await;
        let update_log_1 = create_file(log_dir.path(), "software-update-cmd002").await;
        let update_log_2 = create_file(log_dir.path(), "software-update-cmd003").await;
        let update_log_3 = create_file(log_dir.path(), "software-update-cmd004").await;
        let update_log_4 = create_file(log_dir.path(), "software-update-cmd005").await;
        let swlist_log_2 = create_file(log_dir.path(), "software-list-cmd001").await;
        let update_log_5 = create_file(log_dir.path(), "software-update-cmd006").await;
        let update_log_6 = create_file(log_dir.path(), "software-update-cmd007").await;
        let update_log_7 = create_file(log_dir.path(), "software-update-cmd008").await;

        // Create a new log file
        let new_log = operation_logs
            .new_log_file("software-update".to_string(), "42".to_string())
            .await?;

        // The new log has been created
        assert!(new_log.path.exists());

        // Outdated logs are removed
        assert!(!update_log_1.exists());
        assert!(!update_log_2.exists());
        assert!(!update_log_3.exists());

        // The 5 latest update logs are kept
        assert!(update_log_4.exists());
        assert!(update_log_5.exists());
        assert!(update_log_6.exists());
        assert!(update_log_7.exists());

        // Unrelated logs are kept
        assert!(swlist_log_1.exists());
        assert!(swlist_log_2.exists());

        Ok(())
    }

    async fn create_file(dir: &Path, name: &str) -> PathBuf {
        let file_path = dir.join(name);
        let _log_file = File::create(file_path.clone()).expect("fail to create a test file");
        tokio::time::sleep(Duration::from_millis(5)).await;
        file_path
    }
}
