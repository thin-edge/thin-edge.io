use crate::workflow::GenericStateUpdate;
use crate::workflow::ScriptDefinitionError;
use serde_json::Value;
use std::cmp::max;
use std::fmt::Display;
use std::os::unix::prelude::ExitStatusExt;
use std::time::Duration;

/// Define how to interpret the exit code of a script as the next state for a command
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExitHandlers {
    on_success: Option<GenericStateUpdate>,
    on_error: Option<GenericStateUpdate>,
    on_kill: Option<GenericStateUpdate>,
    on_exit: Vec<(u8, u8, GenericStateUpdate)>,
    on_stdout: Vec<String>,
    timeout: Option<Duration>,
}

impl ExitHandlers {
    pub fn try_new(
        mut on_exit: Vec<(u8, u8, GenericStateUpdate)>,
        mut on_success: Option<GenericStateUpdate>,
        mut on_error: Option<GenericStateUpdate>,
        on_kill: Option<GenericStateUpdate>,
        on_stdout: Vec<String>,
        wildcard: Option<GenericStateUpdate>,
        timeout: Option<Duration>,
    ) -> Result<Self, ScriptDefinitionError> {
        // The on exit error handlers are sorted by range min
        // to ease the implementation of `ExitHandlers::state_update()`
        on_exit.sort_by(|(x, _, _), (y, _, _)| x.cmp(y));

        // The user can provide `on_error` or `on_exit._` but not both
        if let Some(wildcard) = wildcard {
            if on_error.is_some() {
                return Err(ScriptDefinitionError::DuplicatedOnErrorHandler);
            }
            on_error = Some(wildcard)
        }

        // The user can provide `on_success` or `on_exit.0` or even `on_exit.0-x` but not both
        if let Some((0, _, update)) = on_exit.get(0) {
            if on_success.is_some() {
                return Err(ScriptDefinitionError::DuplicatedOnSuccessHandler);
            }
            on_success = Some(update.clone())
        }

        // `on_success` and `on_stdout` are not compatible
        if on_success.is_some() && !on_stdout.is_empty() {
            return Err(ScriptDefinitionError::DuplicatedOnStdoutHandler);
        }

        // Not two ranges can overlap
        let mut previous = None;
        for (from, to, _) in on_exit.iter() {
            if to < from {
                return Err(ScriptDefinitionError::IncorrectRange {
                    from: *from,
                    to: *to,
                });
            }
            if let Some((min, max)) = previous {
                if *from <= max {
                    return Err(ScriptDefinitionError::OverlappingHandler {
                        first: format!("{min}-{max}"),
                        second: format!("{from}-{to}"),
                    });
                }
            }
            previous = Some((*from, *to))
        }

        Ok(ExitHandlers {
            on_success,
            on_error,
            on_kill,
            on_exit,
            on_stdout,
            timeout,
        })
    }

    pub fn state_update(
        &self,
        program: &str,
        outcome: std::io::Result<std::process::Output>,
    ) -> Value {
        match outcome {
            Ok(output) => {
                let json_stdout = json_stdout_excerpt(output.stdout)
                    .context(format!("Program `{program}` stdout"));
                match output.status.code() {
                    None => self
                        .state_update_on_kill(program, output.status.signal().unwrap_or(0) as u8)
                        .into_json(),
                    Some(0) => match (&self.on_success, json_stdout) {
                        (None, Err(reason)) => GenericStateUpdate::failed(reason).into_json(),
                        (None, Ok(dynamic_update)) => dynamic_update,
                        (Some(successful_state), Ok(dynamic_update)) => {
                            successful_state.clone().inject_into_json(dynamic_update)
                        }
                        (Some(successful_state), Err(_)) => successful_state.clone().into_json(),
                    },
                    Some(code) => match self.state_update_on_error(code as u8) {
                        None => self
                            .state_update_on_unknown_exit_code(
                                program,
                                code as u8,
                                json_stdout
                                    .ok()
                                    .and_then(GenericStateUpdate::extract_reason),
                            )
                            .into_json(),
                        Some(error_state) => match json_stdout.ok() {
                            None => error_state.into_json(),
                            Some(dynamic_update) => error_state.inject_into_json(dynamic_update),
                        },
                    },
                }
            }
            Err(err) => self.state_update_on_launch_error(program, err).into_json(),
        }
    }

    pub fn state_update_on_exit(&self, program: &str, code: u8) -> GenericStateUpdate {
        if code == 0 {
            return self.state_update_on_success();
        }

        self.state_update_on_error(code)
            .unwrap_or_else(|| self.state_update_on_unknown_exit_code(program, code, None))
    }

    fn state_update_on_error(&self, code: u8) -> Option<GenericStateUpdate> {
        for (from, to, update) in self.on_exit.iter() {
            if code < *from {
                return None;
            }
            if *from <= code && code <= *to {
                return Some(update.clone());
            }
        }
        None
    }

    pub fn state_update_on_success(&self) -> GenericStateUpdate {
        self.on_success
            .clone()
            .unwrap_or_else(GenericStateUpdate::successful)
    }

    fn state_update_on_launch_error(
        &self,
        program: &str,
        err: std::io::Error,
    ) -> GenericStateUpdate {
        self.on_error.clone().unwrap_or_else(|| {
            GenericStateUpdate::failed(format!("Failed to launch {program}: {err}"))
        })
    }

    fn state_update_on_unknown_exit_code(
        &self,
        program: &str,
        code: u8,
        reason: Option<String>,
    ) -> GenericStateUpdate {
        let mut state = self
            .on_error
            .clone()
            .unwrap_or(GenericStateUpdate::unknown_error());
        state.reason = reason.or(Some(format!("{program} returned exit code {code}")));
        state
    }

    pub fn state_update_on_kill(&self, program: &str, signal: u8) -> GenericStateUpdate {
        self.on_kill.clone().unwrap_or_else(|| {
            GenericStateUpdate::failed(format!("{program} killed by signal {signal}"))
        })
    }

    pub fn graceful_timeout(&self) -> Option<Duration> {
        self.timeout
    }

    pub fn forceful_timeout_extension(&self) -> Option<Duration> {
        self.timeout.map(|timeout| {
            let extra = max(60, timeout.as_secs() / 20);
            Duration::from_secs(extra)
        })
    }
}

/// Extract the json output of a script outcome
pub fn extract_json_output(
    program: &str,
    outcome: std::io::Result<std::process::Output>,
) -> Result<Value, String> {
    json_output(outcome).context(format!("Program `{program}`"))
}

fn json_output(outcome: std::io::Result<std::process::Output>) -> Result<Value, String> {
    let output = outcome.map_err(|err| format!("cannot be launched: {err}"))?;
    let code = output.status.code().ok_or_else(|| {
        format!(
            "has been killed by SIG{}",
            output.status.signal().unwrap_or(0) as u8
        )
    })?;
    if code != 0 {
        return Err(format!("failed with exit code {code}"));
    };

    json_stdout_excerpt(output.stdout).context("stdout")
}

fn json_stdout_excerpt(stdout: Vec<u8>) -> Result<Value, String> {
    String::from_utf8(stdout)
        .map_err(|_| "is not UTF8".to_string())
        .map(extract_script_output)?
        .ok_or_else(|| "contains no :::tedge::: content".to_string())
        .and_then(|excerpt| {
            serde_json::from_str(&excerpt).map_err(|err| format!("is not valid JSON: {err}"))
        })
}

trait WithContext {
    fn context(self, context: impl Display) -> Self;
}

impl<T> WithContext for Result<T, String> {
    fn context(self, context: impl Display) -> Self {
        self.map_err(|err| format!("{context} {err}"))
    }
}

fn extract_script_output(stdout: String) -> Option<String> {
    if let Some((_, script_output_and_more)) = stdout.split_once(":::begin-tedge:::\n") {
        if let Some((script_output, _)) = script_output_and_more.split_once("\n:::end-tedge:::") {
            return Some(script_output.to_string());
        }
    }
    None
}

/// Define how to handle background scripts and actions
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecHandlers {
    pub on_exec: GenericStateUpdate,
}

impl ExecHandlers {
    pub fn try_new(on_exec: Option<GenericStateUpdate>) -> Result<Self, ScriptDefinitionError> {
        Ok(ExecHandlers {
            on_exec: on_exec.unwrap_or_else(GenericStateUpdate::successful),
        })
    }
}

impl ExecHandlers {
    pub fn builtin_default() -> Self {
        ExecHandlers {
            on_exec: GenericStateUpdate::executing(),
        }
    }
}

/// Define how to await the completion of a command
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AwaitHandlers {
    pub timeout: Option<Duration>,
    pub on_success: GenericStateUpdate,
    pub on_error: GenericStateUpdate,
    pub on_timeout: GenericStateUpdate,
}

impl AwaitHandlers {
    pub fn builtin_default() -> Self {
        AwaitHandlers {
            timeout: None,
            on_success: GenericStateUpdate::successful(),
            on_error: GenericStateUpdate::unknown_error(),
            on_timeout: GenericStateUpdate::timeout(),
        }
    }
}

/// Define state transition on each iteration outcome
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IterateHandlers {
    pub on_next: GenericStateUpdate,
    pub on_success: GenericStateUpdate,
    pub on_error: GenericStateUpdate,
}

impl IterateHandlers {
    pub fn new(
        on_next: GenericStateUpdate,
        on_success: GenericStateUpdate,
        on_error: GenericStateUpdate,
    ) -> Self {
        Self {
            on_next,
            on_success,
            on_error,
        }
    }
}

/// Define default handlers for all state of an operation workflow
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefaultHandlers {
    pub timeout: Option<Duration>,
    pub on_error: GenericStateUpdate,
    pub on_timeout: GenericStateUpdate,
}

impl DefaultHandlers {
    pub fn new(
        timeout: Option<Duration>,
        on_error: Option<GenericStateUpdate>,
        on_timeout: Option<GenericStateUpdate>,
    ) -> Self {
        DefaultHandlers {
            timeout,
            on_error: on_error.unwrap_or_else(GenericStateUpdate::unknown_error),
            on_timeout: on_timeout.unwrap_or_else(GenericStateUpdate::timeout),
        }
    }
}

impl Default for DefaultHandlers {
    fn default() -> Self {
        DefaultHandlers {
            timeout: None,
            on_error: GenericStateUpdate::unknown_error(),
            on_timeout: GenericStateUpdate::timeout(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::ShellScript;
    use crate::workflow::OperationAction;
    use crate::workflow::OperationWorkflow;
    use serde_json::json;
    use std::process::Command;
    use std::str::FromStr;

    #[test]
    fn successful_exit_code_determines_next_state() {
        let file = r#"
script = "sh -c 'exit 0'"
on_exit.0 = "yeah"
on_exit._ = "oops"
        "#;
        let (script, handlers) = script_from_toml(file);
        let output = script.output();
        let state_update = handlers.state_update(&script.command, output);
        assert_eq!(
            state_update,
            json! ({
                "status": "yeah"
            })
        )
    }

    #[test]
    fn exit_code_determines_next_state() {
        let file = r#"
script = "sh -c 'exit 3'"
on_exit.0 = "yeah"
on_exit.3 = "got 3"
on_exit._ = "oops"
        "#;
        let (script, handlers) = script_from_toml(file);
        let output = script.output();
        let state_update = handlers.state_update(&script.command, output);
        assert_eq!(
            state_update,
            json! ({
                "status": "got 3"
            })
        )
    }

    #[test]
    fn signal_determines_next_state() {
        let file = r#"
script = "sh -c 'sleep 10'"
on_exit.0 = "yeah"
on_exit._ = "oops"
on_kill = "killed"
        "#;
        let (script, handlers) = script_from_toml(file);
        let mut process = Command::new(script.command.clone())
            .args(script.args.clone())
            .spawn()
            .unwrap();
        let _ = process.kill();
        let output = process.wait_with_output();
        let state_update = handlers.state_update(&script.command, output);
        assert_eq!(
            state_update,
            json! ({
                "status": "killed"
            })
        )
    }

    #[test]
    fn stdout_determines_next_state() {
        let file = r#"
script = "sh -c 'echo :::begin-tedge:::; echo \\{\\\"status\\\": \\\"next\\\"\\}; echo :::end-tedge:::'"
on_stdout = ["next"]
        "#;
        let (script, handlers) = script_from_toml(file);
        let output = script.output();
        let state_update = handlers.state_update(&script.command, output);
        assert_eq!(
            state_update,
            json! ({
                "status": "next"
            })
        )
    }

    #[test]
    fn when_stdout_determines_next_state_it_must_be_provided() {
        // Case where the `:::begin-tedge:::` and `:::end-tedge:::` are missing
        let file = r#"
script = "sh -c 'echo \\{\\\"status\\\": \\\"next\\\"\\}'"
on_stdout = ["next"]
        "#;
        let (script, handlers) = script_from_toml(file);
        let output = script.output();
        let state_update = handlers.state_update(&script.command, output);
        assert_eq!(
            state_update,
            json! ({
                "status": "failed",
                "reason": "Program `sh` stdout contains no :::tedge::: content"
            })
        )
    }

    #[test]
    fn on_success_stdout_is_injected_into_the_next_state() {
        let file = r#"
script = "sh -c 'echo :::begin-tedge:::; echo \\{\\\"foo\\\": \\\"bar\\\"\\}; echo :::end-tedge:::'"
on_success = "yeah"
        "#;
        let (script, handlers) = script_from_toml(file);
        let output = script.output();
        let state_update = handlers.state_update(&script.command, output);
        assert_eq!(
            state_update,
            json! ({
                "status": "yeah",
                "foo": "bar"
            })
        )
    }

    #[test]
    fn on_success_trumps_stdout_status() {
        let file = r#"
script = "sh -c 'echo :::begin-tedge:::; echo \\{\\\"status\\\":\\\"failed\\\", \\\"foo\\\":\\\"bar\\\"\\}; echo :::end-tedge:::'"
on_success = "yeah"
        "#;
        let (script, handlers) = script_from_toml(file);
        let output = script.output();
        let state_update = handlers.state_update(&script.command, output);
        assert_eq!(
            state_update,
            json! ({
                "status": "yeah",
                "foo": "bar"
            })
        )
    }

    #[test]
    fn on_error_stdout_is_ignored() {
        let file = r#"
script = "sh -c 'echo :::begin-tedge:::; echo \\{\\\"foo\\\": \\\"bar\\\"\\}; echo :::end-tedge:::; exit 1'"
on_error = "oops"
        "#;
        let (script, handlers) = script_from_toml(file);
        let output = script.output();
        let state_update = handlers.state_update(&script.command, output);
        assert_eq!(
            state_update,
            json! ({
                "status": "oops",
                "reason": "sh returned exit code 1"
            })
        )
    }

    #[test]
    fn on_error_preserve_failure_reason() {
        let file = r#"
script = "sh -c 'echo :::begin-tedge:::; echo \\{\\\"reason\\\": \\\"No such file or directory\\\"\\}; echo :::end-tedge:::; exit 1'"
on_error = "oops"
        "#;
        let (script, handlers) = script_from_toml(file);
        let output = script.output();
        let state_update = handlers.state_update(&script.command, output);
        assert_eq!(
            state_update,
            json! ({
                "status": "oops",
                "reason": "No such file or directory"
            })
        )
    }

    #[test]
    fn on_expected_exit_code_stdout_reason_trumps_static_reason() {
        let file = r#"
script = "sh -c 'echo :::begin-tedge:::; echo \\{\\\"status\\\":\\\"failed\\\", \\\"reason\\\":\\\"expected\\\", \\\"foo\\\":\\\"bar\\\"\\}; echo :::end-tedge:::; exit 1'"
on_exit.1 = { status = "handle_exit_1", reason = "exit 1"}
on_exit._ = "oops"
        "#;
        let (script, handlers) = script_from_toml(file);
        let output = script.output();
        let state_update = handlers.state_update(&script.command, output);
        assert_eq!(
            state_update,
            json! ({
                "status": "handle_exit_1",
                "reason": "expected",
                "foo": "bar"
            })
        )
    }

    #[test]
    fn on_expected_exit_code_static_reason_is_the_default() {
        let file = r#"
script = "sh -c 'echo :::begin-tedge:::; echo \\{\\\"status\\\":\\\"failed\\\", \\\"foo\\\":\\\"bar\\\"\\}; echo :::end-tedge:::; exit 1'"
on_exit.1 = { status = "handle_exit_1", reason = "exit 1"}
on_exit._ = "oops"
        "#;
        let (script, handlers) = script_from_toml(file);
        let output = script.output();
        let state_update = handlers.state_update(&script.command, output);
        assert_eq!(
            state_update,
            json! ({
                "status": "handle_exit_1",
                "reason": "exit 1",
                "foo": "bar"
            })
        )
    }

    #[test]
    fn json_output_is_extracted_from_script_output() {
        let script = ShellScript::from_str(
            r#"sh -c 'echo :::begin-tedge:::; echo {\"foo\":\"bar\"}; echo :::end-tedge:::'"#,
        )
        .unwrap();
        assert_eq!(
            extract_json_output("user-script", script.output()),
            Ok(json!({"foo":"bar"}))
        );
    }

    #[test]
    fn error_messages_capture_invalid_json() {
        let script = ShellScript::from_str(
            r#"sh -c 'echo :::begin-tedge:::; echo {foo:bar}; echo :::end-tedge:::'"#,
        )
        .unwrap();
        assert_eq!(
            extract_json_output("user-script", script.output()),
            Err("Program `user-script` stdout is not valid JSON: key must be a string at line 1 column 2".to_string())
        );
    }

    #[test]
    fn error_messages_capture_script_exec_errors() {
        let script = ShellScript::from_str("/bin/user-script").unwrap();
        assert_eq!(
            extract_json_output("user-script", script.output()),
            Err(
                "Program `user-script` cannot be launched: No such file or directory (os error 2)"
                    .to_string()
            )
        );
    }

    #[test]
    fn error_messages_capture_script_exit_status() {
        let script = ShellScript::from_str("sh -c 'exit 1'").unwrap();
        assert_eq!(
            extract_json_output("user-script", script.output()),
            Err("Program `user-script` failed with exit code 1".to_string())
        );
    }

    #[test]
    fn error_messages_capture_script_killing_signal() {
        let script = ShellScript::from_str("sh -c 'kill -11 $$'").unwrap();
        assert_eq!(
            extract_json_output("user-script", script.output()),
            Err("Program `user-script` has been killed by SIG11".to_string())
        );
    }

    #[test]
    fn error_messages_capture_ill_formed_script_output() {
        let script = ShellScript::from_str("sh -c 'echo garbage'").unwrap();
        assert_eq!(
            extract_json_output("user-script", script.output()),
            Err("Program `user-script` stdout contains no :::tedge::: content".to_string())
        );
    }

    #[test]
    fn inject_default_values() {
        assert_eq!(
            handlers_from_toml_with_defaults("", ""),
            handlers_from_toml(""),
        );

        assert_eq!(
            handlers_from_toml_with_defaults("", ""),
            handlers_from_toml(
                r#"
on_timeout = { "status" = "failed", reason = "timeout" }
on_error = "failed"
"#
            ),
        );

        assert_eq!(
            handlers_from_toml_with_defaults("", r#"on_success = "ok""#),
            handlers_from_toml(
                r#"
on_success = "ok"
on_timeout = { "status" = "failed", reason = "timeout" }
on_error = "failed"
"#
            ),
        );

        assert_eq!(
            handlers_from_toml_with_defaults(r#"on_error = "broken""#, ""),
            handlers_from_toml(
                r#"
on_timeout = { "status" = "failed", reason = "timeout" }
on_error = "broken"
"#
            ),
        );

        assert_eq!(
            handlers_from_toml_with_defaults(
                "",
                r#"
on_success = "ok"
on_kill = "killed"
on_error = "error"
timeout_second = 15
"#
            ),
            handlers_from_toml(
                r#"
on_success = "ok"
on_kill = "killed"
on_error = "error"
timeout_second = 15
"#
            ),
        );

        assert_eq!(
            handlers_from_toml_with_defaults(
                "timeout_second = 20",
                r#"
on_success = "ok"
on_kill = "killed"
on_error = "error"
timeout_second = 15
"#
            ),
            handlers_from_toml(
                r#"
on_success = "ok"
on_kill = "killed"
on_error = "error"
timeout_second = 15
"#
            ),
        );

        assert_eq!(
            handlers_from_toml_with_defaults(
                "timeout_second = 20",
                r#"
on_success = "ok"
on_kill = "killed"
on_error = "error"
"#
            ),
            handlers_from_toml(
                r#"
on_success = "ok"
on_kill = "killed"
on_error = "error"
timeout_second = 20
"#
            ),
        );

        assert_eq!(
            handlers_from_toml_with_defaults(
                r#"
on_success = "ok"
on_timeout = "timeout"
on_error = "error"
timeout_second = 15
"#,
                r#"
on_success = "ok"
on_kill = "timeout"
on_error = "error"
timeout_second = 15
"#
            ),
            handlers_from_toml(
                r#"
on_success = "ok"
on_kill = "timeout"
on_error = "error"
timeout_second = 15
"#
            ),
        );
    }

    #[test]
    fn exit_handler_timeout() {
        // The forceful timeout extension is 1/20 of the graceful timeout
        // The idea is that we can wait a long time for a command to terminate (more than an hour)
        // But if things are not going to happen we can be forceful.
        let handlers = handlers_from_toml("timeout_second = 1600");
        assert_eq!(handlers.graceful_timeout(), Some(Duration::from_secs(1600)));
        assert_eq!(
            handlers.forceful_timeout_extension(),
            Some(Duration::from_secs(80))
        );

        // The forceful timeout extension is at least 1 minute
        let handlers = handlers_from_toml("timeout_second = 15");
        assert_eq!(handlers.graceful_timeout(), Some(Duration::from_secs(15)));
        assert_eq!(
            handlers.forceful_timeout_extension(),
            Some(Duration::from_secs(60))
        );
    }

    impl ShellScript {
        pub fn output(&self) -> std::io::Result<std::process::Output> {
            Command::new(self.command.clone())
                .args(self.args.clone())
                .output()
        }
    }

    fn script_from_toml(file: &str) -> (ShellScript, ExitHandlers) {
        if let OperationAction::Script(script, handlers) =
            toml::from_str(file).expect("Expect TOML input")
        {
            return (script, handlers);
        }

        panic!("Expect a script with handlers")
    }

    fn handlers_from_toml(file: &str) -> ExitHandlers {
        let (_, handlers) = script_from_toml(&format!(
            r#"
script = "some-script"
{file}
"#
        ));
        handlers
    }

    fn handlers_from_toml_with_defaults(defaults: &str, handlers: &str) -> ExitHandlers {
        let file = format!(
            r#"
operation = "some-operation"
{defaults}

[init]
script = "some-script"
{handlers}
"#
        );
        let workflow: OperationWorkflow = toml::from_str(&file).expect("Expect TOML input");
        if let Some(OperationAction::Script(_, handlers)) = workflow.states.get("init") {
            return handlers.clone();
        }

        panic!("Expect a script with handlers")
    }
}
