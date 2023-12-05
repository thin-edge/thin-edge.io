use crate::workflow::GenericStateUpdate;
use crate::workflow::ScriptDefinitionError;
use serde::de::Error;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExitHandlers {
    on_success: Option<GenericStateUpdate>,
    on_error: Option<GenericStateUpdate>,
    on_kill: Option<GenericStateUpdate>,
    on_exit: Vec<(u8, u8, GenericStateUpdate)>,
}

impl ExitHandlers {
    pub fn try_new(
        mut on_exit: Vec<(u8, u8, GenericStateUpdate)>,
        mut on_success: Option<GenericStateUpdate>,
        mut on_error: Option<GenericStateUpdate>,
        on_kill: Option<GenericStateUpdate>,
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
        })
    }

    pub fn state_update(&self, exit_status: std::process::ExitStatus) -> GenericStateUpdate {
        match exit_status.code() {
            None => self.state_update_on_kill(exit_status.signal().unwrap_or(0) as u8),
            Some(code) => self.state_update_on_exit(code as u8),
        }
    }

    pub fn state_update_on_exit(&self, code: u8) -> GenericStateUpdate {
        if code == 0 {
            return self.state_update_on_success();
        }
        for (from, to, update) in self.on_exit.iter() {
            if code < *from {
                return self.state_update_on_error(code);
            }
            if *from <= code && code <= *to {
                return update.clone();
            }
        }

        self.state_update_on_error(code)
    }

    pub fn state_update_on_success(&self) -> GenericStateUpdate {
        self.on_success
            .clone()
            .unwrap_or_else(|| GenericStateUpdate {
                status: "successful".to_string(),
                reason: None,
            })
    }

    fn state_update_on_error(&self, code: u8) -> GenericStateUpdate {
        self.on_error.clone().unwrap_or_else(|| GenericStateUpdate {
            status: "failed".to_string(),
            reason: Some(format!("returned exit code {code}")),
        })
    }

    pub fn state_update_on_kill(&self, signal: u8) -> GenericStateUpdate {
        self.on_kill.clone().unwrap_or_else(|| GenericStateUpdate {
            status: "failed".to_string(),
            reason: Some(format!("killed by signal {signal}")),
        })
    }
}
