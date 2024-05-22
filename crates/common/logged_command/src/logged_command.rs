use camino::Utf8Path;
use camino::Utf8PathBuf;
use log::error;
use nix::unistd::Pid;
use std::ffi::OsStr;
use std::os::unix::process::ExitStatusExt;
use std::process::Output;
use std::process::Stdio;
use std::time::Duration;
use tedge_api::workflow::CommandId;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::OperationAction;
use tedge_api::workflow::OperationName;
use time::format_description;
use time::OffsetDateTime;
use tokio::fs::File;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
use tokio::io::BufWriter;
use tokio::process::Child;
use tokio::process::Command;

#[derive(Debug)]
pub enum CmdStatus {
    Successful,
    KilledWithSigterm,
    KilledWithSigKill,
}
#[derive(Debug)]
pub struct LoggingChild {
    command_line: String,
    pub inner_child: Child,
}

impl LoggingChild {
    pub async fn wait_for_output_with_timeout(
        self,
        command_log: &mut CommandLog,
        graceful_timeout: Duration,
        forceful_timeout: Duration,
    ) -> Result<Output, std::io::Error> {
        let cid = self.inner_child.id();
        let cmd_line = self.command_line;
        let mut status = CmdStatus::Successful;
        tokio::select! {
            outcome = self.inner_child.wait_with_output() => {
               Self::update_and_log_outcome(cmd_line, outcome, command_log, graceful_timeout, &status).await
            }
            _ = Self::timeout_operation(&mut status, cid, graceful_timeout, forceful_timeout) => {
                Err(std::io::Error::new(std::io::ErrorKind::Other,"failed to kill the process: {cmd_line}"))
            }
        }
    }

    pub async fn wait_with_output(
        self,
        command_log: Option<&mut CommandLog>,
    ) -> Result<Output, std::io::Error> {
        let outcome = self.inner_child.wait_with_output().await;
        if let Some(command_log) = command_log {
            command_log
                .log_command_and_output(&self.command_line, &outcome)
                .await;
        }
        outcome
    }

    async fn update_and_log_outcome(
        command_line: String,
        outcome: Result<Output, std::io::Error>,
        command_log: &mut CommandLog,
        timeout: Duration,
        status: &CmdStatus,
    ) -> Result<Output, std::io::Error> {
        let outcome = match status {
            CmdStatus::Successful => outcome,
            CmdStatus::KilledWithSigterm | CmdStatus::KilledWithSigKill => {
                outcome.map(|outcome| update_stderr_message(outcome, timeout))?
            }
        };
        command_log
            .log_command_and_output(&command_line, &outcome)
            .await;
        outcome
    }

    async fn timeout_operation(
        status: &mut CmdStatus,
        child_id: Option<u32>,
        graceful_timeout: Duration,
        forceful_timeout: Duration,
    ) -> Result<(), std::io::Error> {
        *status = CmdStatus::Successful;

        tokio::time::sleep(graceful_timeout).await;

        // stop the child process by sending sigterm
        *status = CmdStatus::KilledWithSigterm;
        send_signal_to_stop_child(child_id, CmdStatus::KilledWithSigterm);
        tokio::time::sleep(forceful_timeout).await;

        // stop the child process by sending sigkill
        *status = CmdStatus::KilledWithSigKill;
        send_signal_to_stop_child(child_id, CmdStatus::KilledWithSigKill);

        // wait for the process to exit after signal
        tokio::time::sleep(Duration::from_secs(120)).await;

        Ok(())
    }
}

fn update_stderr_message(mut output: Output, timeout: Duration) -> Result<Output, std::io::Error> {
    output.stderr.append(
        &mut format!(
            "operation failed due to timeout: duration={}s",
            timeout.as_secs()
        )
        .as_bytes()
        .to_vec(),
    );
    Ok(output)
}

fn send_signal_to_stop_child(child: Option<u32>, signal_type: CmdStatus) {
    if let Some(pid) = child {
        let pid: Pid = nix::unistd::Pid::from_raw(pid as nix::libc::pid_t);
        match signal_type {
            CmdStatus::KilledWithSigterm => {
                let _ = nix::sys::signal::kill(pid, nix::sys::signal::SIGTERM);
            }
            CmdStatus::KilledWithSigKill => {
                let _ = nix::sys::signal::kill(pid, nix::sys::signal::SIGKILL);
            }
            _ => {}
        }
    }
}

/// A command which execution is logged.
///
/// This struct wraps the main command with a nice representation of that command.
pub struct LoggedCommand {
    command: Command,
}

impl std::fmt::Display for LoggedCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let command = self.command.as_std();

        command.get_program().to_string_lossy().fmt(f)?;
        for arg in command.get_args() {
            // The arguments are displayed as debug, to be properly quoted and distinguished from each other.
            write!(f, " {:?}", arg.to_string_lossy())?;
        }
        Ok(())
    }
}

impl LoggedCommand {
    /// Creates a new `LoggedCommand`.
    ///
    /// In contrast to [`std::process::Command`], `program` can contain space-separated arguments,
    /// which will be properly parsed, split, and passed into `.args()` call for the underlying
    /// command.
    pub fn new(program: impl AsRef<OsStr>) -> Result<LoggedCommand, std::io::Error> {
        let mut args = shell_words::split(&program.as_ref().to_string_lossy())
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;

        let mut command = match args.len() {
            0 => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "command line is empty.",
                ))
            }
            1 => Command::new(&args[0]),
            _ => {
                let mut command = Command::new(args.remove(0));
                command.args(&args);
                command
            }
        };

        command
            // TODO: should use tmp from config
            .current_dir("/tmp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Ok(LoggedCommand { command })
    }

    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut LoggedCommand {
        self.command.arg(arg);
        self
    }

    /// Execute the command and log its exit status, stdout and stderr
    ///
    /// If the command has been executed the outcome is returned (successful or not).
    /// If the command fails to execute (say not found or not executable) an `std::io::Error` is returned.
    ///
    /// If the function fails to log the execution of the command,
    /// this is logged with `log::error!` without changing the return value.
    pub async fn execute(
        mut self,
        command_log: Option<&mut CommandLog>,
    ) -> Result<Output, std::io::Error> {
        let outcome = self.command.output().await;
        if let Some(command_log) = command_log {
            command_log
                .log_command_and_output(&self.to_string(), &outcome)
                .await;
        }
        outcome
    }

    pub fn spawn(&mut self) -> Result<LoggingChild, std::io::Error> {
        let child = self.command.spawn()?;
        Ok(LoggingChild {
            command_line: self.to_string(),
            inner_child: child,
        })
    }

    pub async fn log_outcome(
        command_line: &str,
        result: &Result<Output, std::io::Error>,
        logger: &mut (impl AsyncWrite + Unpin),
    ) -> Result<(), std::io::Error> {
        if !command_line.is_empty() {
            logger
                .write_all(format!("----- $ {}\n", command_line).as_bytes())
                .await?;
        }

        match result.as_ref() {
            Ok(output) => {
                if let Some(code) = &output.status.code() {
                    let exit_code_msg = if *code == 0 { "OK" } else { "ERROR" };
                    logger
                        .write_all(format!("Exit status: {code} ({exit_code_msg})\n\n").as_bytes())
                        .await?
                };
                if let Some(signal) = &output.status.signal() {
                    logger
                        .write_all(format!("Killed by signal: {signal}\n\n").as_bytes())
                        .await?
                }
                // Log stderr then stdout, so the flow reads chronologically
                // as the stderr is used for log messages and the stdout is used for results
                if !output.stderr.is_empty() {
                    logger.write_all(b"stderr <<EOF\n").await?;
                    logger.write_all(&output.stderr).await?;
                    logger.write_all(b"EOF\n\n").await?;
                } else {
                    logger.write_all(b"stderr (EMPTY)\n\n").await?;
                }

                if !output.stdout.is_empty() {
                    logger.write_all(b"stdout <<EOF\n").await?;
                    logger.write_all(&output.stdout).await?;
                    logger.write_all(b"EOF\n").await?;
                } else {
                    logger.write_all(b"stdout (EMPTY)\n").await?;
                }
            }
            Err(err) => {
                logger
                    .write_all(format!("error: {}\n", &err).as_bytes())
                    .await?;
            }
        }

        logger.flush().await?;
        Ok(())
    }
}

impl From<tokio::process::Command> for LoggedCommand {
    fn from(mut command: Command) -> Self {
        command
            // TODO: should use tmp from config
            .current_dir("/tmp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Self { command }
    }
}

impl From<std::process::Command> for LoggedCommand {
    fn from(mut command: std::process::Command) -> Self {
        command
            // TODO: should use tmp from config
            .current_dir("/tmp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Self {
            command: tokio::process::Command::from(command),
        }
    }
}

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
        let operation = &self.operation;
        let parent_operation = if self.invoking_operations.is_empty() {
            operation.to_string()
        } else {
            format!("{} > {}", self.invoking_operations.join(" > "), operation)
        };

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
        let mut command = LoggedCommand::new("echo").unwrap();
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
        let mut command = LoggedCommand::new("ls").unwrap();
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
        let command = LoggedCommand::new("dummy-command").unwrap();

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
