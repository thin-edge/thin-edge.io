//! Execute a command using a shell, and report its output as a workflow script output.
//!
//! This plugin is the executable behind the built-in `shell_execute` operation workflow.
//! It runs `<shell> -c <command>`, captures the combined standard output and standard error
//! of the command, and reports it as the `result` field of the workflow output.

pub mod bin;

use camino::Utf8Path;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;

/// The outcome of a command executed by a shell
#[derive(Debug, Eq, PartialEq)]
pub struct ShellOutcome {
    /// The combined stdout and stderr of the command
    pub result: String,

    /// The exit code of the command
    ///
    /// A command killed by a signal is reported as `128 + signal`, as shells do.
    pub exit_code: i32,
}

/// Run `<shell> -c <command>`, capturing the combined stdout and stderr of the command.
///
/// The output is collected into an unnamed temporary file created in `tmp_dir`,
/// which is shared by stdout and stderr, so that both streams are interleaved
/// the same way a user would see them on a terminal.
///
/// Using a file rather than a pipe matters: a command which starts a background process
/// completes as soon as the command itself exits, even if the background process
/// still holds the inherited file descriptors.
///
/// At most `max_output_size` bytes are read back, so that a chatty command cannot exhaust
/// the memory of a constrained device, nor produce an operation status message
/// too large to be published. Note that this caps the bytes read, not the length of the
/// reported string: invalid UTF-8 is replaced with the unicode replacement character,
/// which is longer than the byte it replaces.
pub fn execute(
    shell: &Utf8Path,
    command: &str,
    tmp_dir: &Utf8Path,
    max_output_size: u32,
) -> std::io::Result<ShellOutcome> {
    let mut output_file = tempfile::tempfile_in(tmp_dir)?;

    let status = Command::new(shell)
        .arg("-c")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::from(output_file.try_clone()?))
        .stderr(Stdio::from(output_file.try_clone()?))
        .status()?;

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

    Ok(ShellOutcome {
        result,
        exit_code: exit_code(status),
    })
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
    let exit_code = outcome.exit_code;
    let prefix = format!("Command returned exit code {exit_code}");

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

    fn sh() -> &'static Utf8Path {
        Utf8Path::new("/bin/sh")
    }

    fn tmp() -> &'static Utf8Path {
        Utf8Path::new("/tmp")
    }

    fn execute_with_defaults(command: &str) -> std::io::Result<ShellOutcome> {
        execute(sh(), command, tmp(), NO_LIMIT)
    }

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
        let outcome = execute_with_defaults("sleep 30 & echo started").unwrap();
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(outcome.result, "started\n");
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
            tmp(),
            NO_LIMIT,
        )
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn a_large_output_is_truncated() {
        let outcome = execute(sh(), "printf '0123456789'", tmp(), 4).unwrap();
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(
            outcome.result,
            "0123\n<the output has been truncated after 4 bytes>\n"
        );
    }

    #[test]
    fn an_output_at_the_limit_is_not_truncated() {
        let outcome = execute(sh(), "printf '0123'", tmp(), 4).unwrap();
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(outcome.result, "0123");
    }

    #[test]
    fn a_character_split_by_the_truncation_is_dropped() {
        // 'é' is 2 bytes long, so the limit of 3 bytes falls inside the second one
        let outcome = execute(sh(), "printf 'éé'", tmp(), 3).unwrap();
        assert_eq!(
            outcome.result,
            "é\n<the output has been truncated after 3 bytes>\n"
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
        let outcome = execute(sh(), "echo the real error; exit 3", tmp(), 20).unwrap();
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
        };
        let mut out = Vec::new();
        write_script_output(&mut out, &outcome).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            ":::begin-tedge:::\n{\"result\":\"a \\\"quoted\\\"\\nvalue\\t!\"}\n:::end-tedge:::\n"
        );
    }
}
