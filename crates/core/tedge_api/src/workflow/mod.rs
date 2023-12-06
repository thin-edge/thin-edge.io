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
pub use script::*;
pub use state::*;
use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Formatter;
pub use supervisor::*;

pub type StateName = String;

/// An OperationWorkflow defines the state machine that rules an operation
#[derive(Clone, Debug)]
pub struct OperationWorkflow {
    /// The operation to which this workflow applies
    pub operation: OperationType,

    /// Mark this workflow as built_in
    pub built_in: bool,

    /// Default action outcome handlers
    pub handlers: ExitHandlers,

    /// The states of the state machine
    pub states: HashMap<StateName, OperationAction>,
}

/// What needs to be done to advance an operation request in some state
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationAction {
    /// Nothing has to be done: simply move to the next step.
    /// Such steps are intended to be overridden.
    ///
    /// ```toml
    /// on_exec = "<state>"
    /// builtin_action = "none"
    /// ```
    MoveTo(StateName),

    /// The built-in behavior is used
    ///
    /// No TOML representation
    BuiltIn,

    /// The command is delegated to a participant identified by its name
    ///
    /// ```toml
    /// builtin_action = "waiting <action>"
    /// ```
    Delegate(String),

    /// Restart the device
    ///
    /// ```toml
    /// builtin_action = "restart"
    /// on_exec = "<state>"
    /// on_success = "<state>"
    /// on_error = "<state>"
    /// ```
    Restart {
        on_exec: StateName,
        on_success: StateName,
        on_error: StateName,
    },

    /// A script has to be executed
    Script(ShellScript, ExitHandlers),

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
            OperationAction::Script(script, _) => script.to_string(),
            OperationAction::Clear => "wait for the requester to finalize the command".to_string(),
        };
        f.write_str(&str)
    }
}

impl OperationWorkflow {
    /// Create a built-in operation workflow
    pub fn built_in(operation: OperationType) -> Self {
        let states = [
            ("init", OperationAction::MoveTo("scheduled".to_string())),
            ("scheduled", OperationAction::BuiltIn),
            ("executing", OperationAction::BuiltIn),
            ("successful", OperationAction::Clear),
            ("failed", OperationAction::Clear),
        ]
        .into_iter()
        .map(|(state, action)| (state.to_string(), action))
        .collect();

        OperationWorkflow {
            built_in: true,
            operation,
            handlers: ExitHandlers::default(),
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
                .map(|action| {
                    let contextualized_action = action.inject_state(&cmd);
                    Some((cmd, contextualized_action))
                }),
            Ok(None) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

impl OperationAction {
    pub fn inject_state(&self, state: &GenericCommandState) -> Self {
        match self {
            OperationAction::Script(script, handlers) => OperationAction::Script(
                ShellScript {
                    command: state.inject_parameter(&script.command),
                    args: state.inject_parameters(&script.args),
                },
                handlers.clone(),
            ),
            _ => self.clone(),
        }
    }
}
