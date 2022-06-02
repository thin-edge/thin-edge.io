use log::error;
use std::{
    ffi::OsStr,
    process::{Output, Stdio},
};
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufWriter},
    process::{Child, Command},
};

#[derive(Debug)]
pub struct LoggingChild {
    command_line: String,
    pub inner_child: Child,
}

impl LoggingChild {
    pub async fn wait_with_output(
        self,
        logger: &mut BufWriter<File>,
    ) -> Result<Output, std::io::Error> {
        let outcome = self.inner_child.wait_with_output().await;
        if let Err(err) = LoggedCommand::log_outcome(&self.command_line, &outcome, logger).await {
            error!("Fail to log the command execution: {}", err);
        }

        outcome
    }
}

/// A command which execution is logged.
///
/// This struct wraps the main command with a nice representation of that command.
/// This `command_line` field is only required because the
/// [`Command::get_program()`](https://doc.rust-lang.org/std/process/struct.Command.html#method.get_program)
/// and
/// [`Command::get_args()`](https://doc.rust-lang.org/std/process/struct.Command.html#method.get_args)
/// are nightly-only experimental APIs.
pub struct LoggedCommand {
    command_line: String,
    command: Command,
}

impl std::fmt::Display for LoggedCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.command_line.fmt(f)
    }
}

impl LoggedCommand {
    pub fn new(program: impl AsRef<OsStr>) -> LoggedCommand {
        let command_line = match program.as_ref().to_str() {
            None => format!("{:?}", program.as_ref()),
            Some(cmd) => cmd.to_string(),
        };

        let mut command = Command::new(program);
        command
            .current_dir("/tmp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        LoggedCommand {
            command_line,
            command,
        }
    }

    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut LoggedCommand {
        // The arguments are displayed as debug, to be properly quoted and distinguished from each other.
        self.command_line.push_str(&format!(" {:?}", arg.as_ref()));
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
    pub async fn execute(mut self, logger: &mut BufWriter<File>) -> Result<Output, std::io::Error> {
        let outcome = self.command.output().await;

        if let Err(err) = LoggedCommand::log_outcome(&self.command_line, &outcome, logger).await {
            error!("Fail to log the command execution: {}", err);
        }

        outcome
    }

    pub fn spawn(&mut self) -> Result<LoggingChild, std::io::Error> {
        let child = self.command.spawn()?;
        Ok(LoggingChild {
            command_line: self.command_line.clone(),
            inner_child: child,
        })
    }

    async fn log_outcome(
        command_line: &str,
        result: &Result<Output, std::io::Error>,
        logger: &mut BufWriter<File>,
    ) -> Result<(), std::io::Error> {
        logger
            .write_all(format!("----- $ {}\n", command_line).as_bytes())
            .await?;

        match result.as_ref() {
            Ok(output) => {
                match &output.status.code() {
                    None => logger.write_all(b"exit status: unknown\n\n").await?,
                    Some(code) => {
                        logger
                            .write_all(format!("exit status: {}\n\n", code).as_bytes())
                            .await?
                    }
                };
                logger.write_all(b"stdout <<EOF\n").await?;
                logger.write_all(&output.stdout).await?;
                logger.write_all(b"EOF\n\n").await?;
                logger.write_all(b"stderr <<EOF\n").await?;
                logger.write_all(&output.stderr).await?;
                logger.write_all(b"EOF\n").await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;
    use tokio::fs::File;

    #[tokio::test]
    async fn on_execute_are_logged_command_line_exit_status_stdout_and_stderr(
    ) -> Result<(), anyhow::Error> {
        // Prepare a log file
        let tmp_dir = TempTedgeDir::new();
        let tmp_file = tmp_dir.file("operation.log");
        let log_file_path = tmp_file.path();
        let log_file = File::create(&log_file_path).await?;
        let mut logger = BufWriter::new(log_file);

        // Prepare a command
        let mut command = LoggedCommand::new("echo");
        command.arg("Hello").arg("World!");

        // Execute the command with logging
        let _ = command.execute(&mut logger).await;

        let log_content = String::from_utf8(std::fs::read(log_file_path)?)?;
        assert_eq!(
            log_content,
            r#"----- $ echo "Hello" "World!"
exit status: 0

stdout <<EOF
Hello World!
EOF

stderr <<EOF
EOF
"#
        );
        Ok(())
    }

    #[tokio::test]
    async fn on_execute_with_error_stderr_is_logged() -> Result<(), anyhow::Error> {
        // Prepare a log file
        let tmp_dir = TempTedgeDir::new();
        let tmp_file = tmp_dir.file("operation.log");
        let log_file_path = tmp_file.path();
        let log_file = File::create(&log_file_path).await?;
        let mut logger = BufWriter::new(log_file);

        // Prepare a command that triggers some content on stderr
        let mut command = LoggedCommand::new("ls");
        command.arg("dummy-file");

        // Execute the command with logging
        let _ = command.execute(&mut logger).await;

        // On expect the errors to be logged
        let log_content = String::from_utf8(std::fs::read(&log_file_path)?)?;
        assert_eq!(
            log_content,
            r#"----- $ ls "dummy-file"
exit status: 2

stdout <<EOF
EOF

stderr <<EOF
ls: cannot access 'dummy-file': No such file or directory
EOF
"#
        );
        Ok(())
    }

    #[tokio::test]
    async fn on_execution_error_are_logged_command_line_and_error() -> Result<(), anyhow::Error> {
        // Prepare a log file
        let tmp_dir = TempTedgeDir::new();
        let tmp_file = tmp_dir.file("operation.log");
        let log_file_path = tmp_file.path();
        let log_file = File::create(&log_file_path).await?;
        let mut logger = BufWriter::new(log_file);

        // Prepare a command that cannot be executed
        let command = LoggedCommand::new("dummy-command");

        // Execute the command with logging
        let _ = command.execute(&mut logger).await;

        // The fact that the command cannot be executed must be logged
        let log_content = String::from_utf8(std::fs::read(&log_file_path)?)?;
        assert_eq!(
            log_content,
            r#"----- $ dummy-command
error: No such file or directory (os error 2)
"#
        );
        Ok(())
    }
}
