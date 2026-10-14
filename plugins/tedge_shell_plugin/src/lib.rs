//! Execute a command using a shell, and report its output as a workflow script output.
//!
//! This plugin is the executable behind the built-in `shell_execute` operation workflow.
//! It runs `<shell> -c <command>`, captures the combined standard output and standard error
//! of the command, and reports it as the `result` field of the workflow output.

pub mod bin;
pub mod job;
mod relay;

use crate::relay::OutputRelay;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::process::Child;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::time::Duration;
use std::time::Instant;

/// The outcome of a command executed by a shell
#[derive(Debug, Eq, PartialEq)]
pub struct ShellOutcome {
    /// The combined stdout and stderr of the command
    pub result: String,

    /// The exit code of the command
    ///
    /// A command killed by a signal is reported as `128 + signal`, as shells do.
    /// A command terminated on timeout is reported as [`TIMEOUT_EXIT_CODE`],
    /// whatever the command returned when terminated.
    pub exit_code: i32,

    /// The timeout, when the command has been terminated for not completing in time
    pub timed_out: Option<Duration>,

    /// Whether the outcome is the result set by the command with `set-result`,
    /// the command having been interrupted, e.g. by a restart of the agent or the device
    pub result_set: bool,

    /// The failure reason set by the command, in place of the one derived from its exit code and output
    pub reason: Option<String>,
}

/// The exit code reported for a command terminated on timeout, as used by the `timeout` utility
pub const TIMEOUT_EXIT_CODE: i32 = 124;

/// How long a command is given to exit after being sent `SIGTERM` on timeout,
/// before being sent `SIGKILL`
///
/// This is the same default as the forceful timeout of the other operations,
/// e.g. the custom operations of the Cumulocity mapper and `tedge diag collect`.
#[cfg(not(test))]
const TERMINATION_GRACE_PERIOD: Duration = Duration::from_secs(60);

/// Shortened, so that the tests of a command ignoring `SIGTERM` do not last a minute
#[cfg(test)]
const TERMINATION_GRACE_PERIOD: Duration = Duration::from_secs(1);

/// The file the output of a command is stored into
pub struct OutputFile {
    pub file: File,

    /// A file created by a process asking for the output relayed so far to be persisted,
    /// and removed once done
    pub flush_request: Option<Utf8PathBuf>,
}

/// Run `<shell> -c <command>`, capturing the combined stdout and stderr of the command,
/// and adding the given environment variables to the environment of the command.
///
/// The output is relayed through a pipe to the given file.
/// The pipe is shared by stdout and stderr, so that both streams are interleaved
/// the same way a user would see them on a terminal.
///
/// Only the first `max_output_size` bytes are stored, so that a chatty command cannot fill up
/// the disk, exhaust the memory of a constrained device, nor produce an operation status message
/// too large to be published. The rest of the output is read and discarded,
/// so the command still runs to completion. Note that this caps the bytes stored, not the length
/// of the reported string: invalid UTF-8 is replaced with the unicode replacement character,
/// which is longer than the byte it replaces.
///
/// The command completes as soon as the shell exits, even if a background process
/// started by the command still holds the pipe. Such a process gets `SIGPIPE`
/// if it writes to its output after the command completed.
///
/// A command which has not completed after `timeout` is terminated,
/// along with the processes it started, and the output collected so far is reported.
/// The shell is the leader of a process group of its own, which is sent `SIGTERM`,
/// then `SIGKILL` if the shell has not exited after a grace period.
/// Only a process which moved to a group or session of its own, e.g. with `setsid`, escapes this.
///
/// The output being written as it is produced, a named file keeps
/// what the command printed, even if this process is killed before the command completes.
/// A process can ask for the output relayed so far to be persisted, e.g. before a reboot,
/// by creating the `flush_request` file, which is removed once done.
pub fn execute_to_file(
    shell: &Utf8Path,
    command: &str,
    output: OutputFile,
    max_output_size: u32,
    timeout: Duration,
    envs: &[(&str, &str)],
) -> std::io::Result<ShellOutcome> {
    let (pipe, pipe_writer) = std::io::pipe()?;
    // The command is dropped once spawned, so this process does not hold the write end of the pipe
    let mut child = Command::new(shell)
        // "--" ends the shell options, so a command starting with a hyphen is run as a command
        .args(["-c", "--", command])
        .envs(envs.iter().copied())
        .stdin(Stdio::null())
        .stdout(pipe_writer.try_clone()?)
        .stderr(pipe_writer)
        .process_group(0)
        .spawn()?;
    let mut relay = OutputRelay::new(pipe, output, max_output_size);

    let deadline = Instant::now() + timeout;
    let (exit_code, timed_out) = match wait_until(&mut child, &mut relay, deadline)? {
        Some(status) => (exit_code(status), None),
        None => {
            terminate(&mut child, &mut relay)?;
            (TIMEOUT_EXIT_CODE, Some(timeout))
        }
    };

    let mut output_file = relay.finish()?;
    let result = read_output(&mut output_file, max_output_size)?;

    Ok(ShellOutcome {
        result,
        exit_code,
        timed_out,
        result_set: false,
        reason: None,
    })
}

/// Read back at most `max_output_size` bytes of the output of a command,
/// adding a notice when the output has been truncated
pub fn read_output(output_file: &mut File, max_output_size: u32) -> std::io::Result<String> {
    let max_output_size = max_output_size as u64;
    let output_size = output_file.metadata()?.len();
    let mut output = Vec::new();
    output_file.rewind()?;
    output_file.take(max_output_size).read_to_end(&mut output)?;

    let truncated = output_size > max_output_size;
    if truncated {
        // Cutting the output at an arbitrary byte can split a multi-byte character in two.
        // `error_len() == None` denotes an incomplete character at the very end of the input,
        // which is dropped rather than reported as invalid.
        if let Err(err) = std::str::from_utf8(&output) {
            if err.error_len().is_none() {
                output.truncate(err.valid_up_to());
            }
        }
    }

    let mut result = String::from_utf8_lossy(&output).into_owned();
    if truncated {
        result.push_str(&format!(
            "\n{TRUNCATION_NOTICE_PREFIX} truncated after {max_output_size} bytes>\n"
        ));
    }

    Ok(result)
}

/// Wait for the child to exit, relaying its output, and giving up at the deadline
///
/// The child is polled, backing off up to a short interval,
/// so a quick command is reported without delay,
/// while a long running and silent one wakes up this process only a few times per second.
/// The output is relayed as soon as it is written, so a chatty command is not slowed down.
fn wait_until(
    child: &mut Child,
    relay: &mut OutputRelay,
    deadline: Instant,
) -> std::io::Result<Option<ExitStatus>> {
    const MAX_POLL_INTERVAL: Duration = Duration::from_millis(100);
    let mut poll_interval = Duration::from_millis(1);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        relay.relay(poll_interval.min(deadline - now))?;
        poll_interval = (poll_interval * 2).min(MAX_POLL_INTERVAL);
    }
}

/// Terminate the process group led by the child, and reap the child
///
/// The output is relayed during the grace period, e.g. what a `SIGTERM` handler prints.
fn terminate(child: &mut Child, relay: &mut OutputRelay) -> std::io::Result<()> {
    use nix::sys::signal::killpg;
    use nix::sys::signal::Signal;
    use nix::unistd::Pid;

    // The child is not reaped yet, so its pid cannot have been reused by another process group
    let process_group = Pid::from_raw(child.id() as nix::libc::pid_t);

    let _ = killpg(process_group, Signal::SIGTERM);
    if wait_until(child, relay, Instant::now() + TERMINATION_GRACE_PERIOD)?.is_none() {
        let _ = killpg(process_group, Signal::SIGKILL);
        child.wait()?;
    }
    Ok(())
}

/// Report a command outcome as a workflow script output
///
/// On success, the workflow engine merges the whole output into the command state,
/// making the command output available as `${.payload.result}`.
///
/// On error, only a `reason` is used by the workflow engine, the rest being discarded.
/// A `reason` is therefore added for a failing command, so the user is told why it failed
/// rather than only being given the exit code.
pub fn write_script_output(out: &mut impl Write, outcome: &ShellOutcome) -> std::io::Result<()> {
    let mut payload = serde_json::json!({ "result": outcome.result });
    // Telling a custom workflow that the command did not complete, but set its result
    if outcome.result_set {
        payload["result_set"] = true.into();
    }
    if outcome.exit_code != 0 {
        payload["reason"] = failure_reason(outcome).into();
    }

    writeln!(out, "{BEGIN_MARKER}")?;
    writeln!(out, "{payload}")?;
    writeln!(out, "{END_MARKER}")
}

/// Report a failure to launch the command as a workflow script output
pub fn write_launch_error(out: &mut impl Write, reason: &str) -> std::io::Result<()> {
    let reason = &reason[..reason.floor_char_boundary(MAX_REASON_LEN)];
    let payload = serde_json::json!({ "reason": reason });
    writeln!(out, "{BEGIN_MARKER}")?;
    writeln!(out, "{payload}")?;
    writeln!(out, "{END_MARKER}")
}

/// The maximum length of a failure reason
///
/// Matching the limit applied by the Cumulocity mapper to an operation failure reason,
/// so the reason is not truncated a second time.
const MAX_REASON_LEN: usize = 500;

/// Describe why a command failed, using its last output line, as this is
/// where a command usually tells what went wrong.
fn failure_reason(outcome: &ShellOutcome) -> String {
    if let Some(reason) = &outcome.reason {
        return reason[..reason.floor_char_boundary(MAX_REASON_LEN)].to_string();
    }

    let prefix = match outcome.timed_out {
        Some(timeout) => format!(
            "Command timed out after {}",
            humantime::format_duration(timeout)
        ),
        None => format!("Command returned exit code {}", outcome.exit_code),
    };

    // Skipping the truncation notice, which is not what the command had to say
    let last_line = outcome
        .result
        .trim_end()
        .lines()
        .rfind(|line| !line.starts_with(TRUNCATION_NOTICE_PREFIX))
        .unwrap_or_default();

    match last_line {
        "" => prefix,
        last_line => {
            let budget = MAX_REASON_LEN.saturating_sub(prefix.len() + 2);
            let last_line = &last_line[..last_line.floor_char_boundary(budget)];
            format!("{prefix}: {last_line}")
        }
    }
}

/// Markers used by the workflow engine to extract the JSON output of a script.
///
/// See `tedge_api::workflow::handlers::extract_script_output`.
const BEGIN_MARKER: &str = ":::begin-tedge:::";
const END_MARKER: &str = ":::end-tedge:::";

/// Prefix of the notice appended to an output which has been truncated
const TRUNCATION_NOTICE_PREFIX: &str = "<the output has been";

#[cfg(unix)]
fn exit_code(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_LIMIT: u32 = u32::MAX;
    const NO_TIMEOUT: Duration = Duration::from_secs(3600);

    #[test]
    fn captures_stdout() {
        let outcome = execute_with_defaults("echo hello world").unwrap();
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(outcome.result, "hello world\n");
    }

    #[test]
    fn captures_stderr_and_exit_code() {
        let outcome = execute_with_defaults("echo oops >&2; exit 3").unwrap();
        assert_eq!(outcome.exit_code, 3);
        assert_eq!(outcome.result, "oops\n");
    }

    #[test]
    fn interleaves_stdout_and_stderr() {
        let outcome = execute_with_defaults("echo one; echo two >&2; echo three").unwrap();
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(outcome.result, "one\ntwo\nthree\n");
    }

    #[test]
    fn a_background_process_does_not_delay_the_command() {
        let started = Instant::now();
        let outcome = execute_with_defaults("sleep 30 & echo started").unwrap();
        // The background process holds the pipe, which is not read to its end
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(outcome.exit_code, 0);
        // Some shells report the termination of the command, e.g. bash
        assert!(
            outcome.result.starts_with("started\n"),
            "{}",
            outcome.result
        );
    }

    #[test]
    fn a_command_starting_with_a_hyphen_is_not_taken_as_a_shell_option() {
        let outcome = execute_with_defaults("-no-such-command 2>/dev/null; echo ran").unwrap();
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(outcome.result, "ran\n");
    }

    #[test]
    fn reports_the_signal_a_command_has_been_killed_with() {
        let outcome = execute_with_defaults("kill -9 $$").unwrap();
        assert_eq!(outcome.exit_code, 137);
    }

    #[test]
    fn missing_shell_is_an_error() {
        let err = execute(
            Utf8Path::new("/no/such/shell"),
            "echo hello",
            NO_LIMIT,
            NO_TIMEOUT,
        )
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn a_large_output_is_truncated() {
        let outcome = execute(sh(), "printf '0123456789'", 4, NO_TIMEOUT).unwrap();
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(
            outcome.result,
            "0123\n<the output has been truncated after 4 bytes>\n"
        );
    }

    #[test]
    fn an_output_at_the_limit_is_not_truncated() {
        let outcome = execute(sh(), "printf '0123'", 4, NO_TIMEOUT).unwrap();
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(outcome.result, "0123");
    }

    #[test]
    fn only_the_head_of_a_large_output_is_stored() {
        let stored = tempfile::NamedTempFile::new_in(tmp()).unwrap();
        let output = OutputFile {
            file: stored.reopen().unwrap(),
            flush_request: None,
        };

        // 10 MB of output, followed by an exit code telling the command ran to completion
        let command = "head -c 10000000 /dev/zero | tr '\\0' x; exit 7";
        let outcome = execute_to_file(sh(), command, output, 4, NO_TIMEOUT, &[]).unwrap();

        assert_eq!(outcome.exit_code, 7);
        assert_eq!(
            outcome.result,
            "xxxx\n<the output has been truncated after 4 bytes>\n"
        );
        // One byte more than reported, telling the output has been truncated
        assert_eq!(stored.as_file().metadata().unwrap().len(), 5);
    }

    #[test]
    fn the_output_relayed_so_far_is_persisted_on_request() {
        let dir = tempfile::tempdir_in(tmp()).unwrap();
        let dir = Utf8Path::from_path(dir.path()).unwrap();
        let stored = dir.join("output");
        let request = dir.join("flush-request");
        let output = OutputFile {
            file: std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&stored)
                .unwrap(),
            flush_request: Some(request.clone()),
        };

        // The command asks for its output to be persisted, as `set-result` does,
        // then prints what has been stored
        let command = format!(
            "echo before; touch '{request}'; while [ -e '{request}' ]; do sleep 0.01; done; cat '{stored}'"
        );
        let outcome = execute_to_file(
            sh(),
            &command,
            output,
            NO_LIMIT,
            Duration::from_secs(10),
            &[],
        )
        .unwrap();

        assert_eq!(outcome.timed_out, None);
        assert_eq!(outcome.result, "before\nbefore\n");
    }

    #[test]
    fn a_character_split_by_the_truncation_is_dropped() {
        // 'é' is 2 bytes long, so the limit of 3 bytes falls inside the second one
        let outcome = execute(sh(), "printf 'éé'", 3, NO_TIMEOUT).unwrap();
        assert_eq!(
            outcome.result,
            "é\n<the output has been truncated after 3 bytes>\n"
        );
    }

    #[test]
    fn a_command_is_terminated_on_timeout() {
        let started = Instant::now();
        let outcome = execute(
            sh(),
            "echo started; sleep 30",
            NO_LIMIT,
            Duration::from_millis(200),
        )
        .unwrap();
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(outcome.exit_code, TIMEOUT_EXIT_CODE);
        assert_eq!(outcome.timed_out, Some(Duration::from_millis(200)));
        // Some shells report the termination of the command, e.g. bash
        assert!(
            outcome.result.starts_with("started\n"),
            "{}",
            outcome.result
        );
    }

    #[test]
    fn the_processes_started_by_a_command_are_terminated_on_timeout() {
        let marker = tempfile::NamedTempFile::new_in(tmp()).unwrap();
        let marker_path = marker.path().to_str().unwrap().to_owned();
        drop(marker);

        // The background process would create the marker file if it was left running
        let command = format!("(sleep 1; touch '{marker_path}') & sleep 30");
        let outcome = execute(sh(), &command, NO_LIMIT, Duration::from_millis(200)).unwrap();
        assert_eq!(outcome.timed_out, Some(Duration::from_millis(200)));

        std::thread::sleep(Duration::from_secs(2));
        assert!(!std::path::Path::new(&marker_path).exists());
    }

    #[test]
    fn a_command_ignoring_sigterm_is_killed_on_timeout() {
        let started = Instant::now();
        let outcome = execute(
            sh(),
            "trap '' TERM; sleep 30",
            NO_LIMIT,
            Duration::from_millis(200),
        )
        .unwrap();
        assert!(started.elapsed() < TERMINATION_GRACE_PERIOD + Duration::from_secs(5));
        assert_eq!(outcome.exit_code, TIMEOUT_EXIT_CODE);
    }

    #[test]
    fn a_command_exiting_successfully_on_sigterm_is_reported_as_failed() {
        let outcome = execute(
            sh(),
            "trap 'exit 0' TERM; echo waiting; while true; do sleep 0.1; done",
            NO_LIMIT,
            Duration::from_millis(200),
        )
        .unwrap();
        let mut out = Vec::new();
        write_script_output(&mut out, &outcome).unwrap();
        assert_eq!(outcome.exit_code, TIMEOUT_EXIT_CODE);
        let out = String::from_utf8(out).unwrap();
        assert!(
            out.contains(r#""reason":"Command timed out after 200ms"#),
            "{out}"
        );
    }

    #[test]
    fn a_failing_command_reports_its_last_output_line_as_the_reason() {
        let outcome = execute_with_defaults("echo ignored; echo oops >&2; exit 3").unwrap();
        let mut out = Vec::new();
        write_script_output(&mut out, &outcome).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            ":::begin-tedge:::\n{\"reason\":\"Command returned exit code 3: oops\",\"result\":\"ignored\\noops\\n\"}\n:::end-tedge:::\n"
        );
    }

    #[test]
    fn a_silent_failing_command_reports_its_exit_code_as_the_reason() {
        let outcome = execute_with_defaults("exit 3").unwrap();
        let mut out = Vec::new();
        write_script_output(&mut out, &outcome).unwrap();
        assert!(String::from_utf8(out)
            .unwrap()
            .contains(r#""reason":"Command returned exit code 3""#));
    }

    #[test]
    fn the_truncation_notice_is_not_used_as_the_failure_reason() {
        let outcome = execute(sh(), "echo the real error; exit 3", 20, NO_TIMEOUT).unwrap();
        let mut out = Vec::new();
        write_script_output(&mut out, &outcome).unwrap();
        assert!(String::from_utf8(out)
            .unwrap()
            .contains(r#""reason":"Command returned exit code 3: the real error""#));
    }

    #[test]
    fn script_output_is_json_escaped() {
        let outcome = ShellOutcome {
            result: "a \"quoted\"\nvalue\t!".to_string(),
            exit_code: 0,
            timed_out: None,
            result_set: false,
            reason: None,
        };
        let mut out = Vec::new();
        write_script_output(&mut out, &outcome).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            ":::begin-tedge:::\n{\"result\":\"a \\\"quoted\\\"\\nvalue\\t!\"}\n:::end-tedge:::\n"
        );
    }

    fn sh() -> &'static Utf8Path {
        Utf8Path::new("/bin/sh")
    }

    fn tmp() -> &'static Utf8Path {
        Utf8Path::new("/tmp")
    }

    fn execute(
        shell: &Utf8Path,
        command: &str,
        max_output_size: u32,
        timeout: Duration,
    ) -> std::io::Result<ShellOutcome> {
        let output = OutputFile {
            file: tempfile::tempfile_in(tmp())?,
            flush_request: None,
        };
        execute_to_file(shell, command, output, max_output_size, timeout, &[])
    }

    fn execute_with_defaults(command: &str) -> std::io::Result<ShellOutcome> {
        execute(sh(), command, NO_LIMIT, NO_TIMEOUT)
    }
}
