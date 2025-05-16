use super::logged_command::LoggedCommand;
use crate::workflow::CommandId;
use crate::workflow::GenericCommandState;
use crate::workflow::OperationAction;
use crate::workflow::OperationName;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use log::error;
use log::info;
use std::process::Output;
use time::format_description;
use time::OffsetDateTime;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::io::BufWriter;

/// Log all command steps
pub struct CommandLog {
    /// Path to the command log file
    pub path: Utf8PathBuf,

    /// operation name
    pub operation: OperationName,

    /// the chain of operations leading to this command
    pub invoking_operations: Vec<OperationName>,

    /// command id
    pub cmd_id: CommandId,

    /// The log file of the root command invoking this command
    ///
    /// None, if not open yet.
    pub file: Option<File>,
}

impl CommandLog {
    pub fn new(
        log_dir: Utf8PathBuf,
        operation: OperationName,
        cmd_id: CommandId,
        invoking_operations: Vec<OperationName>,
        root_operation: Option<OperationName>,
        root_cmd_id: Option<CommandId>,
    ) -> Self {
        let root_operation = root_operation.unwrap_or(operation.clone());
        let root_cmd_id = root_cmd_id.unwrap_or(cmd_id.clone());

        let path = log_dir.join(format!("workflow-{}-{}.log", root_operation, root_cmd_id));
        CommandLog {
            path,
            operation,
            invoking_operations,
            cmd_id,
            file: None,
        }
    }

    pub fn from_log_path(
        path: impl AsRef<Utf8Path>,
        operation: OperationName,
        cmd_id: CommandId,
    ) -> Self {
        Self {
            path: path.as_ref().into(),
            operation,
            cmd_id,
            invoking_operations: vec![],
            file: None,
        }
    }

    pub async fn open(&mut self) -> Result<&mut File, std::io::Error> {
        if self.file.is_none() {
            self.file = Some(
                File::options()
                    .append(true)
                    .create(true)
                    .open(self.path.clone())
                    .await?,
            );
        }
        Ok(self.file.as_mut().unwrap())
    }

    pub async fn log_header(&mut self, topic: &str) {
        let now = OffsetDateTime::now_utc()
            .format(&format_description::well_known::Rfc3339)
            .unwrap();
        let cmd_id = &self.cmd_id;
        let operation = &self.operation;
        let header_message = format!(
            r#"
==================================================================
Triggered {operation} workflow
==================================================================

topic:     {topic}
operation: {operation}
cmd_id:    {cmd_id}
time:      {now}

==================================================================
"#
        );
        if let Err(err) = self.write(&header_message).await {
            error!("Fail to log to {}: {err}", self.path)
        }
    }

    pub async fn log_state_action(
        &mut self,
        state: &GenericCommandState,
        action: &OperationAction,
    ) {
        if state.is_init() && self.invoking_operations.is_empty() {
            self.log_header(state.topic.name.as_str()).await;
        }
        let step = &state.status;
        let state = &state.payload.to_string();
        let message = format!(
            r#"
State:    {state}

Action:   {action}
"#
        );
        self.log_step(step, &message).await
    }

    pub async fn log_step(&mut self, step: &str, action: &str) {
        let now = OffsetDateTime::now_utc()
            .format(&format_description::well_known::Rfc3339)
            .unwrap();
        let parent_operation = self.invoking_chain();

        let message = format!(
            r#"
----------------------[ {parent_operation} @ {step} | time={now} ]----------------------
{action}
"#
        );
        if let Err(err) = self.write(&message).await {
            error!("Fail to log to {}: {err}", self.path)
        }
    }

    fn invoking_chain(&self) -> String {
        let operation = &self.operation;
        if self.invoking_operations.is_empty() {
            operation.to_string()
        } else {
            format!("{} > {}", self.invoking_operations.join(" > "), operation)
        }
    }

    pub async fn log_next_step(&mut self, step: &str) {
        let context = self.invoking_chain();
        self.log_info(&format!("=> moving to {context} @ {step}"))
            .await
    }

    pub async fn log_script_output(&mut self, result: &Result<Output, std::io::Error>) {
        self.log_command_and_output("", result).await
    }

    pub async fn log_command_and_output(
        &mut self,
        command_line: &str,
        result: &Result<Output, std::io::Error>,
    ) {
        if let Err(err) = self.write_script_output(command_line, result).await {
            error!("Fail to log to {}: {err}", self.path)
        }
    }

    pub async fn log_info(&mut self, msg: &str) {
        info!("{msg}");
        let line = format!("{msg}\n");
        if let Err(err) = self.write(&line).await {
            error!("Fail to log to {}: {err}", self.path)
        }
    }

    pub async fn log_error(&mut self, msg: &str) {
        error!("{msg}");
        let line = format!("ERROR: {msg}\n");
        if let Err(err) = self.write(&line).await {
            error!("Fail to log to {}: {err}", self.path)
        }
    }

    pub async fn write_script_output(
        &mut self,
        command_line: &str,
        result: &Result<Output, std::io::Error>,
    ) -> Result<(), std::io::Error> {
        let file = self.open().await?;
        let mut writer = BufWriter::new(file);
        LoggedCommand::log_outcome(command_line, result, &mut writer).await?;
        Ok(())
    }

    pub async fn write(&mut self, message: impl AsRef<[u8]>) -> Result<(), std::io::Error> {
        let file = self.open().await?;
        file.write_all(message.as_ref()).await?;
        file.flush().await?;
        file.sync_all().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn on_execute_are_logged_command_line_exit_status_stdout_and_stderr(
    ) -> Result<(), anyhow::Error> {
        // Prepare a log file
        let tmp_dir = TempTedgeDir::new();
        let workflow_log = tmp_dir.file("workflow.log");
        let log_file_path = workflow_log.path();
        let mut command_log = CommandLog::from_log_path(
            workflow_log.utf8_path(),
            "software_update".into(),
            "123".into(),
        );

        // Prepare a command
        let mut command = LoggedCommand::new("echo", "/tmp").unwrap();
        command.arg("Hello").arg("World!");

        // Execute the command with logging
        let _ = command.execute(Some(&mut command_log)).await;

        let log_content = String::from_utf8(std::fs::read(log_file_path)?)?;
        assert_eq!(
            log_content,
            r#"----- $ echo "Hello" "World!"
Exit status: 0 (OK)

stderr (EMPTY)

stdout <<EOF
Hello World!
EOF
"#
        );
        Ok(())
    }

    #[tokio::test]
    async fn on_execute_with_error_stderr_is_logged() -> Result<(), anyhow::Error> {
        // Prepare a log file
        let tmp_dir = TempTedgeDir::new();
        let workflow_log = tmp_dir.file("workflow.log");
        let log_file_path = workflow_log.path();
        let mut command_log = CommandLog::from_log_path(
            workflow_log.utf8_path(),
            "software_update".into(),
            "123".into(),
        );

        // Prepare a command that triggers some content on stderr
        let mut command = LoggedCommand::new("ls", "/tmp").unwrap();
        command.arg("dummy-file");

        // Execute the command with logging
        let _ = command.execute(Some(&mut command_log)).await;

        // On expect the errors to be logged
        let log_content = String::from_utf8(std::fs::read(log_file_path)?)?;
        #[cfg(target_os = "linux")]
        assert_eq!(
            log_content,
            r#"----- $ ls "dummy-file"
Exit status: 2 (ERROR)

stderr <<EOF
ls: cannot access 'dummy-file': No such file or directory
EOF

stdout (EMPTY)
"#
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            log_content,
            r#"----- $ ls "dummy-file"
Exit status: 1 (ERROR)

stderr <<EOF
ls: dummy-file: No such file or directory
EOF

stdout (EMPTY)
"#
        );
        Ok(())
    }

    #[tokio::test]
    async fn on_execution_error_are_logged_command_line_and_error() -> Result<(), anyhow::Error> {
        // Prepare a log file
        let tmp_dir = TempTedgeDir::new();
        let workflow_log = tmp_dir.file("workflow.log");
        let log_file_path = workflow_log.path();
        let mut command_log = CommandLog::from_log_path(
            workflow_log.utf8_path(),
            "software_update".into(),
            "123".into(),
        );

        // Prepare a command that cannot be executed
        let command = LoggedCommand::new("dummy-command", "/tmp").unwrap();

        // Execute the command with logging
        let _ = command.execute(Some(&mut command_log)).await;

        // The fact that the command cannot be executed must be logged
        let log_content = String::from_utf8(std::fs::read(log_file_path)?)?;
        assert_eq!(
            log_content,
            r#"----- $ dummy-command
error: No such file or directory (os error 2)
"#
        );
        Ok(())
    }
}
