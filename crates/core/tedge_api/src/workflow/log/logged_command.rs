use crate::CommandLog;
use std::ffi::OsStr;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::Output;
use std::process::Stdio;
use std::time::Duration;
use tedge_utils::signals::terminate_process;
use tedge_utils::signals::Signal;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
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
                Err(std::io::Error::other(format!("failed to kill the process: {cmd_line}")))
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

        if let Some(pid) = child_id {
            tokio::time::sleep(graceful_timeout).await;

            // stop the child process by sending sigterm
            *status = CmdStatus::KilledWithSigterm;
            terminate_process(pid, Signal::SIGTERM);

            tokio::time::sleep(forceful_timeout).await;

            // stop the child process by sending sigkill
            *status = CmdStatus::KilledWithSigKill;
            terminate_process(pid, Signal::SIGKILL);
        }

        // wait for the process to exit after signal
        tokio::time::sleep(Duration::from_secs(120)).await;

        Ok(())
    }
}

fn update_stderr_message(mut output: Output, timeout: Duration) -> Result<Output, std::io::Error> {
    output.stderr.append(
        &mut format!(
            "operation failed due to timeout: duration={}s\n",
            timeout.as_secs()
        )
        .as_bytes()
        .to_vec(),
    );
    Ok(output)
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
    pub fn new(
        program: impl AsRef<OsStr>,
        working_dir: impl AsRef<Path>,
    ) -> Result<LoggedCommand, std::io::Error> {
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
            .current_dir(working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Ok(LoggedCommand { command })
    }

    pub fn from_command(mut command: std::process::Command, working_dir: impl AsRef<Path>) -> Self {
        command
            .current_dir(working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Self {
            command: tokio::process::Command::from(command),
        }
    }

    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut LoggedCommand {
        self.command.arg(arg);
        self
    }

    pub fn args<T: AsRef<OsStr>, TS: IntoIterator<Item = T>>(
        &mut self,
        args: TS,
    ) -> &mut LoggedCommand {
        self.command.args(args);
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
