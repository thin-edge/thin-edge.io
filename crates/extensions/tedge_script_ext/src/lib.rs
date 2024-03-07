pub use shell_words::ParseError;
use std::process::Output;
use std::process::Stdio;
use std::time::Duration;
use tedge_actors::Concurrent;
use tedge_actors::Server;
use tedge_actors::ServerActorBuilder;
use tedge_actors::ServerConfig;

#[derive(Clone)]
pub struct ScriptActor;

#[derive(Debug, Eq, PartialEq)]
pub struct Execute {
    pub command: String,
    pub args: Vec<String>,
    pub timeouts: Option<(Duration, Duration)>,
}

impl Execute {
    /// A new command with its arguments
    pub fn new(command: String, args: Vec<String>) -> Self {
        Execute {
            command,
            args,
            timeouts: None,
        }
    }

    /// Parse the command line into a program and its arguments
    pub fn try_new(command_line: &str) -> Result<Self, ParseError> {
        let mut args = shell_words::split(command_line)?;
        if args.is_empty() {
            Err(ParseError)
        } else {
            let command = args.remove(0);
            Ok(Execute::new(command, args))
        }
    }

    /// Give the process a graceful timeout to run, timeout after which a SIGTERM is sent
    pub fn with_graceful_timeout(self, graceful_timeout: Duration) -> Self {
        let timeouts = match self.timeouts {
            None => (graceful_timeout, Duration::from_secs(5)),
            Some((_, forceful_timeout)) => (graceful_timeout, forceful_timeout),
        };
        Self {
            timeouts: Some(timeouts),
            ..self
        }
    }

    /// Give the process an extra forceful timeout to exit on SIGTERM, timeout after which a SIGTKILL is sent
    pub fn with_forceful_timeout_extension(self, forceful_timeout: Duration) -> Self {
        let timeouts = match self.timeouts {
            None => (Duration::from_secs(15), forceful_timeout),
            Some((graceful_timeout, _)) => (graceful_timeout, forceful_timeout),
        };
        Self {
            timeouts: Some(timeouts),
            ..self
        }
    }
}

#[async_trait::async_trait]
impl Server for ScriptActor {
    type Request = Execute;
    type Response = std::io::Result<Output>;

    fn name(&self) -> &str {
        "Script"
    }

    async fn handle(&mut self, message: Self::Request) -> Self::Response {
        let child = tokio::process::Command::new(message.command)
            .args(message.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        match (child.id(), message.timeouts) {
            (_, None) | (None, _) => child.wait_with_output().await,
            (Some(pid), Some((graceful_timeout, forceful_timeout))) => {
                tokio::select! {
                    response = child.wait_with_output() => response,
                    not_killed = kill_on_timeout(pid, graceful_timeout, forceful_timeout) => Err(not_killed),
                }
            }
        }
    }
}

async fn kill_on_timeout(
    pid: u32,
    graceful_timeout: Duration,
    forceful_timeout: Duration,
) -> std::io::Error {
    let pid = nix::unistd::Pid::from_raw(pid as nix::libc::pid_t);

    tokio::time::sleep(graceful_timeout).await;
    let _ = nix::sys::signal::kill(pid, nix::sys::signal::SIGTERM);

    tokio::time::sleep(forceful_timeout).await;
    let _ = nix::sys::signal::kill(pid, nix::sys::signal::SIGKILL);

    tokio::time::sleep(Duration::from_secs(1)).await;
    std::io::Error::new(
        std::io::ErrorKind::Other,
        "failed to kill the process after timeout",
    )
}

impl ScriptActor {
    pub fn builder() -> ServerActorBuilder<ScriptActor, Concurrent> {
        ServerActorBuilder::new(ScriptActor, &ServerConfig::default(), Concurrent)
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt;
    use std::time::Duration;
    use tedge_actors::ClientMessageBox;

    use super::*;

    #[test]
    fn test_parsing() {
        assert_eq!(
            Execute::try_new(r#"python -c "print('Hello world!')""#),
            Ok(Execute {
                command: "python".to_string(),
                args: vec!["-c".to_string(), "print('Hello world!')".to_string()],
                timeouts: None,
            })
        )
    }

    #[tokio::test]
    async fn script() {
        let mut handle = spawn_script_actor();
        let output = handle
            .await_response(Execute {
                command: "echo".to_owned(),
                args: vec!["A message".to_owned()],
                timeouts: None,
            })
            .await
            .unwrap()
            .unwrap();

        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "A message\n");
        assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    }

    #[tokio::test]
    async fn script_stdin_is_closed() {
        let mut actor = spawn_script_actor();
        let command = Execute::try_new("cat").unwrap();
        let output = tokio::time::timeout(Duration::from_secs(1), actor.await_response(command))
            .await
            .expect("execution timeout")
            .expect("result send error")
            .expect("execution error");

        assert!(output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }

    #[tokio::test]
    async fn script_is_given_enough_time() {
        let mut actor = spawn_script_actor();
        let command = Execute::try_new("echo hello world")
            .unwrap()
            .with_graceful_timeout(Duration::from_secs(1));
        let output = tokio::time::timeout(Duration::from_secs(5), actor.await_response(command))
            .await
            .expect("execution timeout")
            .expect("result send error")
            .expect("execution error");

        assert!(output.status.success());
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.status.signal(), None);
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "hello world\n");
    }

    #[tokio::test]
    async fn script_is_gracefully_killed() {
        let mut actor = spawn_script_actor();
        let command = Execute::try_new("sleep 10")
            .unwrap()
            .with_graceful_timeout(Duration::from_secs(1));
        let output = tokio::time::timeout(Duration::from_secs(5), actor.await_response(command))
            .await
            .expect("execution timeout")
            .expect("result send error")
            .expect("execution error");

        assert!(!output.status.success());
        assert!(output.status.code().is_none());
        assert_eq!(output.status.signal(), Some(15));
    }

    #[tokio::test]
    async fn script_is_forcefully_killed() {
        let mut actor = spawn_script_actor();
        let command_line = r#"/usr/bin/env bash -c "trap 'echo ignore SIGTERM' SIGTERM; while true; do sleep 1; done""#;
        let command = Execute::try_new(command_line)
            .unwrap()
            .with_graceful_timeout(Duration::from_secs(1))
            .with_forceful_timeout_extension(Duration::from_secs(1));
        let output = tokio::time::timeout(Duration::from_secs(5), actor.await_response(command))
            .await
            .expect("execution timeout")
            .expect("result send error")
            .expect("execution error");

        assert!(!output.status.success());
        assert!(output.status.code().is_none());
        assert_eq!(output.status.signal(), Some(9));
    }

    fn spawn_script_actor() -> ClientMessageBox<Execute, std::io::Result<Output>> {
        let mut actor = ScriptActor::builder();
        let handle = ClientMessageBox::new(&mut actor);
        tokio::spawn(actor.run());
        handle
    }
}
