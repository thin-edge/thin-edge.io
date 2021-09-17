use std::process::{Stdio, Output};
use tokio::fs::File;
use tokio::io::{BufWriter, AsyncWriteExt};
use tokio::process::Command;

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

    pub fn new(program: &str) -> LoggedCommand {
        let command_line = program.to_string();
        let mut command = Command::new(program);
        command
            .current_dir("/tmp")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        LoggedCommand { command_line, command }
    }

    pub fn arg(&mut self, arg: &str) -> &mut LoggedCommand {
        self.command_line.push_str(&format!(" {:?}", arg));
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
        let outcome = self.command
            .output()
            .await;

        if let Err(err) = LoggedCommand::log_outcome(&self.command_line, &outcome, logger).await {
            log::error!("Fail to log the command execution: {}", err);
        }

        outcome
    }

    async fn log_outcome (
        command_line: &str,
        result: &Result<Output, std::io::Error>,
        logger: &mut BufWriter<File>,
    ) -> Result<(), std::io::Error> {
        logger
            .write_all(format!("----- $ {}\n", command_line).as_bytes())
            .await?;

        match result.as_ref() {
            Ok(output) => {
                logger
                    .write_all(format!("{}\n\n", &output.status).as_bytes())
                    .await?;
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
    use tempfile::*;
    use tokio::fs::File;

    #[tokio::test]
    async fn on_execute_are_logged_command_line_exit_status_stdout_and_stderr() -> Result<(), anyhow::Error> {
        // Prepare a log file
        let tmp_dir = TempDir::new()?;
        let log_file_path = tmp_dir.path().join("operation.log");
        let log_file = File::create(log_file_path.clone()).await?;
        let mut logger = BufWriter::new(log_file);

        // Prepare a command
        let mut command = LoggedCommand::new("echo");
        command
            .arg("Hello")
            .arg("World!");

        // Execute the command with logging
        let _ = command.execute(&mut logger).await;

        let log_content = String::from_utf8(std::fs::read(&log_file_path)?)?;
        assert_eq!(
            log_content,
            r#"----- $ echo "Hello" "World!"
exit code: 0

stdout <<EOF
Hello World!
EOF

stderr <<EOF
EOF
"#
        );
        Ok(())
    }
}
