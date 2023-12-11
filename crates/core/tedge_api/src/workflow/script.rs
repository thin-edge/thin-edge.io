use crate::workflow::GenericStateUpdate;
use crate::workflow::ScriptDefinitionError;
use serde::de::Error;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde_json::Value;
use std::cmp::min;
use std::fmt::Display;
use std::fmt::Formatter;
use std::os::unix::prelude::ExitStatusExt;
use std::time::Duration;

/// A parsed Unix command line
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellScript {
    pub command: String,
    pub args: Vec<String>,
}

/// Deserialize an Unix command line
impl<'de> Deserialize<'de> for ShellScript {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let command_line = String::deserialize(deserializer)?;
        let mut args = shell_words::split(&command_line)
            .map_err(|err| D::Error::custom(format!("invalid script: {command_line}: {err}")))?;
        if args.is_empty() {
            Err(D::Error::custom("invalid script: empty"))
        } else {
            let script = args.remove(0);
            Ok(ShellScript {
                command: script,
                args,
            })
        }
    }
}

/// Serialize an Unix command line, using appropriate quotes
impl Serialize for ShellScript {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl Display for ShellScript {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut args = vec![self.command.clone()];
        args.append(&mut self.args.clone());
        f.write_str(&shell_words::join(args))
    }
}

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

    pub fn with_default(mut self, default: &DefaultHandlers) -> Self {
        if self.timeout.is_none() {
            self.timeout = default.timeout
        }
        if self.on_kill.is_none() {
            self.on_kill = default.on_timeout.clone()
        }
        if self.on_error.is_none() {
            self.on_error = default.on_error.clone()
        }

        self
    }

    pub fn state_update(
        &self,
        program: &str,
        outcome: std::io::Result<std::process::Output>,
    ) -> Value {
        match outcome {
            Ok(output) => match output.status.code() {
                None => self
                    .state_update_on_kill(program, output.status.signal().unwrap_or(0) as u8)
                    .into_json(),
                Some(0) => {
                    match (
                        &self.on_success,
                        self.json_stdout_excerpt(program, output.stdout),
                    ) {
                        (None, Err(reason)) => GenericStateUpdate::failed(reason).into_json(),
                        (None, Ok(dynamic_update)) => dynamic_update,
                        (Some(successful_state), Ok(dynamic_update)) => {
                            successful_state.clone().inject_into_json(dynamic_update)
                        }
                        (Some(successful_state), Err(_)) => successful_state.clone().into_json(),
                    }
                }
                Some(code) => match self.state_update_on_error(code as u8) {
                    None => self
                        .state_update_on_unknown_exit_code(program, code as u8)
                        .into_json(),
                    Some(error_state) => {
                        match self.json_stdout_excerpt(program, output.stdout).ok() {
                            None => error_state.into_json(),
                            Some(dynamic_update) => error_state.inject_into_json(dynamic_update),
                        }
                    }
                },
            },
            Err(err) => self.state_update_on_launch_error(program, err).into_json(),
        }
    }

    pub fn state_update_on_exit(&self, program: &str, code: u8) -> GenericStateUpdate {
        if code == 0 {
            return self.state_update_on_success();
        }

        self.state_update_on_error(code)
            .unwrap_or_else(|| self.state_update_on_unknown_exit_code(program, code))
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

    fn json_stdout_excerpt(&self, program: &str, stdout: Vec<u8>) -> Result<Value, String> {
        match String::from_utf8(stdout) {
            Err(_) => Err(format!("{program} returned no UTF8 stdout")),
            Ok(content) => match extract_script_output(content) {
                None => Err(format!(
                    "{program} returned no :::tedge::: content on stdout"
                )),
                Some(excerpt) => match serde_json::from_str(&excerpt) {
                    Ok(json) => Ok(json),
                    Err(err) => Err(format!(
                        "{program} returned non JSON content on stdout: {err}"
                    )),
                },
            },
        }
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

    fn state_update_on_unknown_exit_code(&self, program: &str, code: u8) -> GenericStateUpdate {
        self.on_error.clone().unwrap_or_else(|| {
            GenericStateUpdate::failed(format!("{program} returned exit code {code}"))
        })
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
            let extra = min(60, timeout.as_secs() / 20);
            Duration::from_secs(extra)
        })
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

/// Define how to handle a background script
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BgExitHandlers {
    pub on_exec: GenericStateUpdate,
}

impl BgExitHandlers {
    pub fn try_new(on_exec: Option<GenericStateUpdate>) -> Result<Self, ScriptDefinitionError> {
        Ok(BgExitHandlers {
            on_exec: on_exec.unwrap_or_else(GenericStateUpdate::successful),
        })
    }
}

/// Define default handlers for all state of an operation workflow
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DefaultHandlers {
    pub timeout: Option<Duration>,
    pub on_timeout: Option<GenericStateUpdate>,
    pub on_error: Option<GenericStateUpdate>,
}

impl DefaultHandlers {
    pub fn try_new(
        timeout: Option<Duration>,
        on_timeout: Option<GenericStateUpdate>,
        on_error: Option<GenericStateUpdate>,
    ) -> Result<Self, ScriptDefinitionError> {
        Ok(DefaultHandlers {
            timeout,
            on_timeout,
            on_error,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::OperationAction;
    use serde_json::json;
    use std::process::Command;

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
                "reason": "sh returned no :::tedge::: content on stdout"
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
                "status": "oops"
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
}
