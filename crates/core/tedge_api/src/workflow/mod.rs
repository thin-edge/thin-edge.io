pub mod error;
pub mod script;
pub mod state;
pub mod supervisor;
pub mod toml_config;

use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::MqttSchema;
use crate::mqtt_topics::OperationType;
pub use error::*;
use mqtt_channel::Message;
use mqtt_channel::QoS;
use script::ShellScript;
pub use script::*;
use serde::Deserialize;
use serde::Serialize;
pub use state::*;
use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Formatter;
pub use supervisor::*;

pub type StateName = String;
pub type OperationName = String;

/// An OperationWorkflow defines the state machine that rules an operation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationWorkflow {
    /// The operation to which this workflow applies
    pub operation: OperationType,

    /// Mark this workflow as built_in
    #[serde(default, skip)]
    pub built_in: bool,

    /// The states of the state machine
    #[serde(flatten)]
    pub states: HashMap<StateName, OperationState>,
}

/// The current state of an operation request
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationState {
    /// Possibly a participant to which the action is delegated
    pub owner: Option<String>,

    /// Possibly a script to handle the operation when in that state
    pub script: Option<ShellScript>,

    /// Transitions
    pub next: Vec<StateName>,
}

/// What needs to be done to advance an operation request in some state
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationAction {
    /// Nothing has to be done: simply move to the next step.
    /// Such steps are intended to be overridden.
    MoveTo(StateName),

    /// The built-in behavior is used
    BuiltIn,

    /// The command is delegated to a participant identified by its name
    Delegate(String),

    /// Restart the device
    Restart {
        on_exec: StateName,
        on_success: StateName,
        on_error: StateName,
    },

    /// A script has to be executed
    Script(ShellScript),

    /// The command has been fully processed and needs to be cleared
    Clear,
}

impl Display for OperationAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            OperationAction::MoveTo(step) => format!("move to {step} state"),
            OperationAction::BuiltIn => "builtin".to_string(),
            OperationAction::Delegate(owner) => {
                format!("wait for {owner} to perform required actions")
            }
            OperationAction::Restart { .. } => "trigger device restart".to_string(),
            OperationAction::Script(script) => script.to_string(),
            OperationAction::Clear => "wait for the requester to finalize the command".to_string(),
        };
        f.write_str(&str)
    }
}

impl OperationWorkflow {
    /// Create a built-in operation workflow
    pub fn built_in(operation: OperationType) -> Self {
        let states = [
            ("init", false, vec!["scheduled"]),
            ("scheduled", true, vec!["executing"]),
            ("executing", true, vec!["successful", "failed"]),
            ("successful", false, vec![]),
            ("failed", false, vec![]),
        ]
        .into_iter()
        .map(|(step, delegate, next)| {
            (
                step.to_string(),
                OperationState {
                    owner: if delegate {
                        Some("tedge".to_string())
                    } else {
                        None
                    },
                    script: None,
                    next: next.into_iter().map(|s| s.to_string()).collect(),
                },
            )
        })
        .collect();

        OperationWorkflow {
            built_in: true,
            operation,
            states,
        }
    }

    /// Return the MQTT message to register support for the operation described by this workflow
    pub fn capability_message(&self, schema: &MqttSchema, target: &EntityTopicId) -> Message {
        let meta_topic = schema.capability_topic_for(target, self.operation.clone());
        let payload = "{}";
        Message::new(&meta_topic, payload)
            .with_retain()
            .with_qos(QoS::AtLeastOnce)
    }

    /// Extract the current action to be performed on a command request
    ///
    /// Returns:
    /// - `Ok(Some(action)` when the request is well-formed
    /// - `Ok(None)` when the request is finalized, i.e. when the command topic hase been cleared
    /// - `Err(error)` when the request is ill-formed
    pub fn get_operation_current_action(
        &self,
        message: &Message,
    ) -> Result<Option<(GenericCommandState, OperationAction)>, WorkflowExecutionError> {
        match GenericCommandState::from_command_message(message) {
            Ok(Some(cmd)) => self
                .states
                .get(&cmd.status)
                .ok_or_else(|| WorkflowExecutionError::UnknownStep {
                    operation: (&self.operation).into(),
                    step: cmd.status.clone(),
                })
                .map(|state| {
                    let action = OperationAction::from(state).inject_state(&cmd);
                    Some((cmd, action))
                }),
            Ok(None) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

impl From<&OperationState> for OperationAction {
    // TODO this must be called when an operation is registered, not when invoked.
    fn from(state: &OperationState) -> Self {
        match &state.script {
            Some(script) if script.command == "restart" => {
                let (on_exec, on_success, on_error) = match &state.next[..] {
                    [] => ("executing", "successful", "failed"),
                    [restarting] => (restarting.as_ref(), "successful", "failed"),
                    [restarting, successful] => {
                        (restarting.as_ref(), successful.as_ref(), "failed")
                    }
                    [restarting, successful, failed, ..] => {
                        (restarting.as_ref(), successful.as_ref(), failed.as_str())
                    }
                };
                OperationAction::Restart {
                    on_exec: on_exec.to_string(),
                    on_success: on_success.to_string(),
                    on_error: on_error.to_string(),
                }
            }
            Some(script) => OperationAction::Script(script.clone()),
            None => match &state.owner {
                Some(owner) if owner == "tedge" => OperationAction::BuiltIn,
                Some(owner) => OperationAction::Delegate(owner.to_owned()),
                None => match &state.next[..] {
                    [] => OperationAction::Clear,
                    [next] => OperationAction::MoveTo(next.to_owned()),
                    _ => OperationAction::Delegate("unknown".to_string()),
                },
            },
        }
    }
}

impl OperationAction {
    pub fn inject_state(self, state: &GenericCommandState) -> Self {
        match self {
            OperationAction::Script(script) => OperationAction::Script(ShellScript {
                command: state.inject_parameter(&script.command),
                args: state.inject_parameters(&script.args),
            }),
            _ => self,
        }
    }
}
