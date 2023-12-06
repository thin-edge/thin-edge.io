use crate::workflow::GenericStateUpdate;
use crate::workflow::ScriptDefinitionError;
use serde::de::Error;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde_json::Value;
use std::fmt::Display;
use std::fmt::Formatter;
use std::os::unix::prelude::ExitStatusExt;

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
}

impl ExitHandlers {
    pub fn try_new(
        mut on_exit: Vec<(u8, u8, GenericStateUpdate)>,
        mut on_success: Option<GenericStateUpdate>,
        mut on_error: Option<GenericStateUpdate>,
        on_kill: Option<GenericStateUpdate>,
        on_stdout: Vec<String>,
        wildcard: Option<GenericStateUpdate>,
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
            if !on_stdout.is_empty() {
                return Err(ScriptDefinitionError::DuplicatedOnStdoutHandler);
            }
            on_success = Some(update.clone())
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
        })
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
                    match (&self.on_success, self.json_stdout_excerpt(program, output.stdout)) {
                        (None, Err(reason)) => GenericStateUpdate::failed(reason).into_json(),
                        (None, Ok(json_update)) => json_update,
                        (Some(successful_state), Ok(json_update)) => successful_state.clone().inject_into_json(json_update),
                        (Some(successful_state), Err(_)) => successful_state.clone().into_json(),
                    }
                }
                Some(code) => self.state_update_on_exit(program, code as u8).into_json(),
            },
            Err(err) => self.state_update_on_error(program, err).into_json(),
        }
    }

    pub fn state_update_on_exit(&self, program: &str, code: u8) -> GenericStateUpdate {
        if code == 0 {
            return self.state_update_on_success();
        }
        for (from, to, update) in self.on_exit.iter() {
            if code < *from {
                return self.state_update_on_unknown_exit_code(program, code);
            }
            if *from <= code && code <= *to {
                return update.clone();
            }
        }

        self.state_update_on_unknown_exit_code(program, code)
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

    fn state_update_on_error(&self, program: &str, err: std::io::Error) -> GenericStateUpdate {
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
}

fn extract_script_output(stdout: String) -> Option<String> {
    if let Some((_, script_output_and_more)) = stdout.split_once(":::begin-tedge:::\n") {
        if let Some((script_output, _)) = script_output_and_more.split_once("\n:::end-tedge:::") {
            return Some(script_output.to_string());
        }
    }
    None
}
