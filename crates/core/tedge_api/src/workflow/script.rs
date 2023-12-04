use crate::workflow::GenericStateUpdate;
use crate::workflow::ScriptDefinitionError;
use crate::workflow::TomlStateUpdate;
use serde::de::Error;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Write;
use std::num::ParseIntError;
use std::os::unix::prelude::ExitStatusExt;
use std::str::FromStr;

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

impl TryFrom<TomlExitHandlers> for ExitHandlers {
    type Error = ScriptDefinitionError;

    fn try_from(value: TomlExitHandlers) -> Result<Self, Self::Error> {
        let on_error = value.on_error.map(|u| u.into());
        let on_success = value.on_success.map(|u| u.into());
        let on_kill = value.on_kill.map(|u| u.into());
        let wildcard = value
            .on_exit
            .get(&ExitCodes::AnyError)
            .map(|u| u.clone().into());
        let on_exit: Vec<(u8, u8, GenericStateUpdate)> = value
            .on_exit
            .into_iter()
            .filter_map(|(code, state)| {
                let state = state.into();
                match code {
                    ExitCodes::Code(x) => Some((x, x, state)),
                    ExitCodes::Range { from, to } => Some((from, to, state)),
                    ExitCodes::AnyError => None,
                }
            })
            .collect();

        ExitHandlers::try_new(on_exit, on_success, on_error, on_kill, wildcard)
    }
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

/// User-Friendly representation of an [ExitHandlers]; as used in the operation TOML definition files
///
/// A user don't have to give a handler for all possible exit code.
/// - A handler can simply be a string used as the next state for the command.
/// - A handler can be attached to a range of exit code
/// - A wildcard handler can be defined as a default handler
/// - `on_success` is syntactic sugar for `on_exit.0`
/// - `on_error` is syntactic sugar for `on_exit._`
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TomlExitHandlers {
    #[serde(skip_serializing_if = "Option::is_none")]
    on_success: Option<TomlStateUpdate>,

    #[serde(skip_serializing_if = "Option::is_none")]
    on_error: Option<TomlStateUpdate>,

    #[serde(skip_serializing_if = "Option::is_none")]
    on_kill: Option<TomlStateUpdate>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    on_exit: HashMap<ExitCodes, TomlStateUpdate>,
}

/// Represent either:
/// - a specific exit code
/// - a range of exit codes
/// - any non-zero code
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum ExitCodes {
    Code(u8),
    Range { from: u8, to: u8 },
    AnyError,
}

impl<'de> Deserialize<'de> for ExitCodes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let exit_code = String::deserialize(deserializer)?;
        exit_code
            .parse()
            .map_err(|err| D::Error::custom(format!("invalid exit: {exit_code}: {err}")))
    }
}

impl Serialize for ExitCodes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl Display for ExitCodes {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ExitCodes::Code(x) => x.fmt(f),
            ExitCodes::Range { from, to } => {
                from.fmt(f)?;
                f.write_char('-')?;
                to.fmt(f)
            }
            ExitCodes::AnyError => f.write_char('_'),
        }
    }
}

impl FromStr for ExitCodes {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "_" {
            return Ok(ExitCodes::AnyError);
        }

        match s.split_once('-') {
            None => Ok(ExitCodes::Code(s.parse()?)),
            Some((from, to)) => Ok(ExitCodes::Range {
                from: from.parse()?,
                to: to.parse()?,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::GenericStateUpdate;
    use ExitCodes::*;

    #[test]
    fn parse_exit_handlers() {
        let file = r#"
on_exit.0 = "next_state"                                  # next state for an exit status
on_exit.1 = { status = "retry_state", reason = "busy"}    # next status with fields
on_exit.2-5 = { status = "fatal_state", reason = "oops"}  # next state for a range of exit status
on_exit._ = "failed"                                      # wildcard for any other non successfull exit
on_kill = { status = "failed", reason = "killed"}         # next status when killed
        "#;
        let input: TomlExitHandlers = toml::from_str(file).unwrap();
        assert_eq!(
            input,
            TomlExitHandlers {
                on_success: None,
                on_error: None,
                on_kill: Some(TomlStateUpdate::Detailed(GenericStateUpdate {
                    status: "failed".to_string(),
                    reason: Some("killed".to_string())
                })),
                on_exit: HashMap::from_iter([
                    (Code(0), TomlStateUpdate::Simple("next_state".to_string())),
                    (
                        Code(1),
                        TomlStateUpdate::Detailed(GenericStateUpdate {
                            status: "retry_state".to_string(),
                            reason: Some("busy".to_string())
                        })
                    ),
                    (
                        Range { from: 2, to: 5 },
                        TomlStateUpdate::Detailed(GenericStateUpdate {
                            status: "fatal_state".to_string(),
                            reason: Some("oops".to_string())
                        })
                    ),
                    (AnyError, TomlStateUpdate::Simple("failed".to_string())),
                ])
            }
        )
    }

    #[test]
    fn get_state_update_from_exit_status() {
        let file = r#"
on_exit.0 = "0"
on_exit.3-5 = "3-5"
on_exit._ = "wildcard"
on_kill = "killed"
on_exit.1 = "1"
        "#;
        let input: TomlExitHandlers = toml::from_str(file).unwrap();
        let handlers: ExitHandlers = input.try_into().unwrap();
        assert_eq!(handlers.state_update_on_success().status, "0");
        assert_eq!(handlers.state_update_on_exit(0).status, "0");
        assert_eq!(handlers.state_update_on_exit(1).status, "1");
        assert_eq!(handlers.state_update_on_exit(2).status, "wildcard");
        assert_eq!(handlers.state_update_on_exit(3).status, "3-5");
        assert_eq!(handlers.state_update_on_exit(4).status, "3-5");
        assert_eq!(handlers.state_update_on_exit(5).status, "3-5");
        assert_eq!(handlers.state_update_on_exit(6).status, "wildcard");
        assert_eq!(handlers.state_update_on_kill(9).status, "killed");
    }

    #[test]
    fn forbid_duplicated_success_handler() {
        let file = r#"
on_exit.0 = "0"
on_success = "success"
        "#;
        let input: TomlExitHandlers = toml::from_str(file).unwrap();
        let error = TryInto::<ExitHandlers>::try_into(input).unwrap_err();
        assert_eq!(error, ScriptDefinitionError::DuplicatedOnSuccessHandler)
    }

    #[test]
    fn forbid_duplicated_error_handler() {
        let file = r#"
on_exit._ = "wildcard"
on_error = "error"
        "#;
        let input: TomlExitHandlers = toml::from_str(file).unwrap();
        let error = TryInto::<ExitHandlers>::try_into(input).unwrap_err();
        assert_eq!(error, ScriptDefinitionError::DuplicatedOnErrorHandler)
    }

    #[test]
    fn forbid_overlapping_error_handler() {
        let file = r#"
on_exit.1-5 = "1-5"
on_exit.4-8 = "4-8"
        "#;
        let input: TomlExitHandlers = toml::from_str(file).unwrap();
        let error = TryInto::<ExitHandlers>::try_into(input).unwrap_err();
        assert_eq!(
            error,
            ScriptDefinitionError::OverlappingHandler {
                first: "1-5".to_string(),
                second: "4-8".to_string()
            }
        )
    }

    #[test]
    fn forbid_ill_defined_range() {
        let file = r#"
on_exit.5-1 = "oops"
        "#;
        let input: TomlExitHandlers = toml::from_str(file).unwrap();
        let error = TryInto::<ExitHandlers>::try_into(input).unwrap_err();
        assert_eq!(
            error,
            ScriptDefinitionError::IncorrectRange { from: 5, to: 1 }
        )
    }

    #[test]
    fn default_handlers() {
        let file = "";
        let input: TomlExitHandlers = toml::from_str(file).unwrap();
        let handlers = TryInto::<ExitHandlers>::try_into(input).unwrap();
        assert_eq!(handlers.state_update_on_success().status, "successful");
        assert_eq!(
            handlers.state_update_on_exit(1).reason.unwrap(),
            "returned exit code 1"
        );
        assert_eq!(
            handlers.state_update_on_kill(9).reason.unwrap(),
            "killed by signal 9"
        );
    }
}
