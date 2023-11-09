use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::MqttSchema;
use crate::mqtt_topics::OperationType;
use mqtt_channel::Message;
use mqtt_channel::QoS;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

pub type StateName = String;
pub type OperationName = String;

/// An OperationWorkflow defines the state machine that rules an operation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationWorkflow {
    /// The operation to which this workflow applies
    pub operation: OperationType,

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
    pub script: Option<String>,

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

    /// A script has to be executed
    Script(String),

    /// The command has been fully processed and needs to be cleared
    Clear,
}

/// Error preventing a workflow to be registered
#[derive(thiserror::Error, Debug)]
pub enum WorkflowDefinitionError {
    #[error("Missing mandatory state: {state}")]
    MissingState { state: StateName },

    #[error("Missing transition for state: {state}")]
    MissingTransition { state: StateName },
}

/// Error preventing a workflow to be registered
#[derive(thiserror::Error, Debug)]
pub enum WorkflowRegistrationError {
    #[error("A workflow for this operation is already registered: {operation}")]
    DuplicatedWorkflow { operation: OperationName },
}

/// Error preventing to infer the current action for an operation instance
#[derive(thiserror::Error, Debug)]
pub enum WorkflowExecutionError {
    #[error("The command payload is not a JSON object")]
    InvalidPayload(#[from] serde_json::Error),

    #[error("Missing status in the command payload")]
    MissingStatus,

    #[error("No workflow is defined for the operation: {operation}")]
    UnknownOperation { operation: OperationName },

    #[error("No such step is defined for {operation}: {step}")]
    UnknownStep {
        operation: OperationName,
        step: StateName,
    },
}

/// Dispatch actions to operation participants
#[derive(Default)]
pub struct WorkflowSupervisor {
    /// The user-defined operation workflow definitions
    workflows: HashMap<OperationType, OperationWorkflow>,
}

impl WorkflowSupervisor {
    /// Register a builtin workflow provided by thin-edge
    pub fn register_builtin_workflow(
        &mut self,
        operation: OperationType,
    ) -> Result<(), WorkflowRegistrationError> {
        self.register_custom_workflow(OperationWorkflow::built_in(operation))
    }

    /// Register a user-defined workflow
    pub fn register_custom_workflow(
        &mut self,
        workflow: OperationWorkflow,
    ) -> Result<(), WorkflowRegistrationError> {
        if self.workflows.contains_key(&workflow.operation) {
            Err(WorkflowRegistrationError::DuplicatedWorkflow {
                operation: workflow.operation.to_string(),
            })
        } else {
            self.workflows.insert(workflow.operation.clone(), workflow);
            Ok(())
        }
    }

    /// List the capabilities provided by the registered workflows
    pub fn capability_messages(&self, schema: &MqttSchema, target: &EntityTopicId) -> Vec<Message> {
        // To ease testing the capability messages are emitted in a deterministic order
        let mut operations = self.workflows.values().collect::<Vec<_>>();
        operations.sort_by(|&a, &b| a.operation.to_string().cmp(&b.operation.to_string()));
        operations
            .iter()
            .map(|workflow| workflow.capability_message(schema, target))
            .collect()
    }

    /// Extract the current action to be performed on a command request
    ///
    /// Returns:
    /// - `Ok(Some(action)` when the request is well-formed
    /// - `Ok(None)` when the request is finalized, i.e. when the command topic hase been cleared
    /// - `Err(error)` when the request is ill-formed
    pub fn get_workflow_current_action(
        &self,
        operation: &OperationType,
        status: &Message,
    ) -> Result<Option<(GenericCommandState, OperationAction)>, WorkflowExecutionError> {
        self.workflows
            .get(operation)
            .ok_or_else(|| WorkflowExecutionError::UnknownOperation {
                operation: operation.into(),
            })
            .and_then(|workflow| OperationWorkflow::get_operation_current_action(workflow, status))
    }
}

impl OperationWorkflow {
    /// Create a built-in operation workflow
    pub fn built_in(operation: OperationType) -> Self {
        let states = [
            ("init", vec!["executing"]),
            ("executing", vec!["successful", "failed"]),
            ("successful", vec![]),
            ("failed", vec![]),
        ]
        .into_iter()
        .map(|(step, next)| {
            (
                step.to_string(),
                OperationState {
                    owner: None,
                    script: None,
                    next: next.into_iter().map(|s| s.to_string()).collect(),
                },
            )
        })
        .collect();

        OperationWorkflow { operation, states }
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
                .map(|state| Some((cmd, OperationAction::from(state)))),
            Ok(None) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

impl From<&OperationState> for OperationAction {
    // TODO this must be called when an operation is registered, not when invoked.
    fn from(state: &OperationState) -> Self {
        match &state.script {
            Some(script) => OperationAction::Script(script.to_owned()),
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

/// Generic command state that can be used to manipulate any type of command payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenericCommandState {
    pub status: String,
    pub json: Value,
}

impl GenericCommandState {
    /// Extract a command state from a json payload
    pub fn from_command_message(message: &Message) -> Result<Option<Self>, WorkflowExecutionError> {
        let payload = message.payload_bytes();
        if payload.is_empty() {
            return Ok(None);
        }
        let json: Value = serde_json::from_slice(payload)?;
        let status = GenericCommandState::extract_text_property(&json, "status")
            .ok_or(WorkflowExecutionError::MissingStatus)?;
        Ok(Some(GenericCommandState { status, json }))
    }

    /// Serialize the command state as a json payload
    pub fn to_json_string(mut self) -> String {
        GenericCommandState::inject_text_property(&mut self.json, "status", &self.status);
        self.json.to_string()
    }

    /// Inject a json payload into this one
    pub fn update_from_json(mut self, json: Value) -> Self {
        if let (Some(values), Some(new_values)) = (self.json.as_object_mut(), json.as_object()) {
            for (k, v) in new_values {
                values.insert(k.to_string(), v.clone());
            }
        }
        match GenericCommandState::extract_text_property(&self.json, "status") {
            None => self.fail_with("Unknown status".to_string()),
            Some(status) => GenericCommandState { status, ..self },
        }
    }

    /// Update the command state with the outcome of a script
    pub fn update_with_script_output(
        self,
        script: String,
        output: std::io::Result<std::process::Output>,
    ) -> Self {
        match output {
            Ok(output) => {
                if output.status.success() {
                    match String::from_utf8(output.stdout) {
                        Ok(stdout) => match serde_json::from_str(&stdout) {
                            Ok(json) => self.update_from_json(json),
                            Err(err) => {
                                let reason =
                                    format!("Script {script} returned non JSON stdout: {err}");
                                self.fail_with(reason)
                            }
                        },
                        Err(_) => {
                            let reason = format!("Script {script} returned non UTF-8 stdout");
                            self.fail_with(reason)
                        }
                    }
                } else {
                    match String::from_utf8(output.stderr) {
                        Ok(stderr) => {
                            let reason = format!("Script {script} failed with: {stderr}");
                            self.fail_with(reason)
                        }
                        Err(_) => {
                            let reason =
                                format!("Script {script} failed and returned non UTF-8 stderr");
                            self.fail_with(reason)
                        }
                    }
                }
            }
            Err(err) => {
                let reason = format!("Failed to launch {script}: {err}");
                self.fail_with(reason)
            }
        }
    }

    /// Update the command state with a new status describing the next state
    pub fn move_to(mut self, status: String) -> Self {
        GenericCommandState::inject_text_property(&mut self.json, "status", &status);

        GenericCommandState { status, ..self }
    }

    /// Update the command state to failed status with the given reason
    pub fn fail_with(mut self, reason: String) -> Self {
        let status = "failed";
        GenericCommandState::inject_text_property(&mut self.json, "status", status);
        GenericCommandState::inject_text_property(&mut self.json, "reason", &reason);

        GenericCommandState {
            status: status.to_owned(),
            ..self
        }
    }

    /// Extract a text property from a Json object
    fn extract_text_property(json: &Value, property: &str) -> Option<String> {
        json.as_object()
            .and_then(|o| o.get(property))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Inject a text property into a Json object
    fn inject_text_property(json: &mut Value, property: &str, value: &str) {
        if let Some(o) = json.as_object_mut() {
            o.insert(property.to_string(), value.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_channel::Topic;
    use serde_json::json;

    #[test]
    fn serde_generic_command_payload() {
        let topic = Topic::new_unchecked("te/device/main///cmd/make_it/123");
        let payload = r#"{ "status":"init", "foo":42, "bar": { "extra": [1,2,3] }}"#;
        let command = mqtt_channel::Message::new(&topic, payload);
        let cmd = GenericCommandState::from_command_message(&command)
            .expect("parsing error")
            .expect("no message");
        assert_eq!(
            cmd,
            GenericCommandState {
                status: "init".to_string(),
                json: json!({
                    "status": "init",
                    "foo": 42,
                    "bar": {
                        "extra": [1,2,3]
                    }
                })
            }
        );

        let update_cmd = cmd.move_to("executing".to_string());
        assert_eq!(
            update_cmd,
            GenericCommandState {
                status: "executing".to_string(),
                json: json!({
                    "status": "executing",
                    "foo": 42,
                    "bar": {
                        "extra": [1,2,3]
                    }
                })
            }
        );

        let final_cmd = update_cmd.fail_with("panic".to_string());
        assert_eq!(
            final_cmd,
            GenericCommandState {
                status: "failed".to_string(),
                json: json!({
                    "status": "failed",
                    "reason": "panic",
                    "foo": 42,
                    "bar": {
                        "extra": [1,2,3]
                    }
                })
            }
        );
    }
}
