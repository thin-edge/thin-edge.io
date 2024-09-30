pub mod error;
pub(crate) mod log;
mod on_disk;
pub mod script;
pub mod state;
pub mod supervisor;
mod toml_config;

use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::MqttSchema;
use crate::mqtt_topics::OperationType;
use ::log::info;
pub use error::*;
use mqtt_channel::MqttMessage;
use mqtt_channel::QoS;
pub use script::*;
use serde::Deserialize;
use serde_json::json;
pub use state::*;
use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Formatter;
pub use supervisor::*;

pub type OperationName = String;
pub type StateName = String;
pub type CommandId = String;
pub type JsonPath = String;

/// An OperationWorkflow defines the state machine that rules an operation
#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "toml_config::TomlOperationWorkflow")]
pub struct OperationWorkflow {
    /// The operation to which this workflow applies
    pub operation: OperationType,

    /// Mark this workflow as built_in
    pub built_in: bool,

    /// Default action outcome handlers
    pub handlers: DefaultHandlers,

    /// The states of the state machine
    pub states: HashMap<StateName, OperationAction>,
}

/// What needs to be done to advance an operation request in some state
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(try_from = "toml_config::TomlOperationState")]
pub enum OperationAction {
    /// Nothing has to be done: simply move to the next step.
    /// Such steps are intended to be overridden.
    ///
    /// ```toml
    /// action = "proceed"
    /// on_success = "<state>"
    /// ```
    MoveTo(GenericStateUpdate),

    /// Implied built-in operation (for backward compatibility)
    ///
    /// - the operation name is derived from the workflow
    /// - the step (trigger vs await) is derived from the command status (scheduled vs executing)
    ///
    /// ```toml
    /// action = "builtin"
    /// on_exec = "<state>"
    /// on_success = "<state>"
    /// on_error = "<state>"
    /// ```
    BuiltIn(ExecHandlers, AwaitHandlers),

    /// Await agent restart
    ///
    /// In practice, this command simply waits till a timeout.
    /// If the timeout triggers, this step fails.
    /// If the agent stops before the timeout and finds on restart a persisted state of `await-agent-restart`,
    /// then this step is successful.
    ///
    /// ```toml
    /// action = "await-agent-restart"
    /// on_success = "<state>"
    /// on_error = "<state>"
    /// ```
    AwaitingAgentRestart(AwaitHandlers),

    /// A script has to be executed
    Script(ShellScript, ExitHandlers),

    /// Executes a script but move to the next state without waiting for that script to return
    ///
    /// Notably such a script can trigger a device reboot or an agent restart.
    /// ```toml
    /// background_script = "sudo systemctl restart tedge-agent"
    /// on_exec = "<state>"
    /// ```
    BgScript(ShellScript, ExecHandlers),

    /// Trigger an operation and move to the next state from where the outcome of the operation will be awaited
    ///
    /// ```toml
    /// operation = "sub_operation"
    /// input_script = "/path/to/sub_operation/input_scrip.sh ${.payload.x}" ${.payload.y}"
    /// input.logfile = "${.payload.logfile}"
    /// on_exec = "awaiting_sub_operation"
    /// ```
    Operation(
        OperationName,
        Option<ShellScript>,
        StateExcerpt,
        ExecHandlers,
    ),

    /// Trigger a built-in operation
    ///
    /// ```toml
    /// operation = "<builtin:operation-name>"
    /// on_exec = "<state>"
    /// ```
    BuiltInOperation(OperationName, ExecHandlers),

    /// Await the completion of a sub-operation
    ///
    /// The sub-operation is stored in the command state.
    ///
    /// ```toml
    /// action = "await-operation-completion"
    /// on_success = "<state>"
    /// on_error = "<state>"
    /// ```
    AwaitOperationCompletion(AwaitHandlers, StateExcerpt),

    /// The command has been fully processed and needs to be cleared
    Clear,

    /// Extract the next item from the specified target array in the state payload.
    /// The next item is captured into a `@next` fragment in the state payload output,
    /// with an `index` field having an initial value of zero.
    /// If the input already contains the `@next` fragment with an `index` value,
    /// that index is incremented and the corresponding value from the array is
    /// extracted as the next item into the `@next` fragment.
    ///
    /// ```toml
    /// iterate = "${.payload.operations}"
    /// on_next = "apply_operation"
    /// on_success = "successful"
    /// on_error = "failed"
    /// ```
    Iterate(JsonPath, IterateHandlers),
}

impl Display for OperationAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            OperationAction::MoveTo(step) => format!("move to {step} state"),
            OperationAction::BuiltIn(_, _) => "builtin action".to_string(),
            OperationAction::AwaitingAgentRestart { .. } => "await agent restart".to_string(),
            OperationAction::Script(script, _) => script.to_string(),
            OperationAction::BgScript(script, _) => script.to_string(),
            OperationAction::Operation(operation, maybe_script, _, _) => match maybe_script {
                None => format!("execute {operation} as sub-operation"),
                Some(script) => format!(
                    "execute {operation} as sub-operation, with input payload derived from: {}",
                    script
                ),
            },
            OperationAction::BuiltInOperation(operation, _) => {
                format!("execute builtin:{operation}")
            }
            OperationAction::AwaitOperationCompletion { .. } => {
                "await sub-operation completion".to_string()
            }
            OperationAction::Clear => "wait for the requester to finalize the command".to_string(),
            OperationAction::Iterate(json_path, _) => {
                format!("iterate over {json_path}").to_string()
            }
        };
        f.write_str(&str)
    }
}

impl OperationWorkflow {
    /// Return a new OperationWorkflow unless there are errors
    /// such as missing or ill-defined states.
    pub fn try_new(
        operation: OperationType,
        handlers: DefaultHandlers,
        mut states: HashMap<StateName, OperationAction>,
    ) -> Result<Self, WorkflowDefinitionError> {
        // The init state is required
        if !states.contains_key("init") {
            return Err(WorkflowDefinitionError::MissingState {
                state: "init".to_string(),
            });
        }

        // The successful state can be omitted,
        // but must be associated to a `clear` if provided.
        let action_on_success = states
            .entry("successful".to_string())
            .or_insert(OperationAction::Clear);
        if action_on_success != &OperationAction::Clear {
            return Err(WorkflowDefinitionError::InvalidAction {
                state: "successful".to_string(),
                action: format!("{action_on_success:?}"),
            });
        }

        // The failed state can be omitted,
        // but must be associated to a `clear` if provided.
        let action_on_error = states
            .entry("failed".to_string())
            .or_insert(OperationAction::Clear);
        if action_on_error != &OperationAction::Clear {
            return Err(WorkflowDefinitionError::InvalidAction {
                state: "failed".to_string(),
                action: format!("{action_on_error}"),
            });
        }

        let main_operation = operation.to_string();
        for (_, action) in states.iter() {
            match action {
                // A `builtin:<operation>` can only be invoked from the same `<operation>`
                OperationAction::BuiltInOperation(builtin_operation, _)
                    if builtin_operation != &main_operation =>
                {
                    return Err(WorkflowDefinitionError::InvalidBuiltinOperation {
                        main_operation,
                        builtin_operation: builtin_operation.clone(),
                    })
                }
                _ => continue,
            }
        }

        Ok(OperationWorkflow {
            operation,
            built_in: false,
            handlers,
            states,
        })
    }

    /// Create a built-in operation workflow
    pub fn built_in(operation: OperationType) -> Self {
        let operation_name = operation.to_string();
        let exec_handler = ExecHandlers::builtin_default();
        let await_handler = AwaitHandlers::builtin_default();
        let states = [
            ("init", OperationAction::MoveTo("scheduled".into())),
            (
                "scheduled",
                OperationAction::BuiltInOperation(operation_name.clone(), exec_handler),
            ),
            (
                "executing",
                OperationAction::AwaitOperationCompletion(
                    await_handler,
                    StateExcerpt::whole_payload(),
                ),
            ),
            ("successful", OperationAction::Clear),
            ("failed", OperationAction::Clear),
        ]
        .into_iter()
        .map(|(state, action)| (state.to_string(), action))
        .collect();

        OperationWorkflow {
            built_in: true,
            operation,
            handlers: DefaultHandlers::default(),
            states,
        }
    }

    /// Create a workflow that systematically fail any command with a static error
    ///
    /// The point is to raise an error to the user when a workflow definition cannot be parsed,
    /// instead of silently ignoring the commands.
    pub fn ill_formed(operation: String, reason: String) -> Self {
        let states = [
            ("init", OperationAction::MoveTo("executing".into())),
            (
                "executing",
                OperationAction::MoveTo(GenericStateUpdate::failed(reason)),
            ),
            ("failed", OperationAction::Clear),
        ]
        .into_iter()
        .map(|(state, action)| (state.to_string(), action))
        .collect();

        OperationWorkflow {
            built_in: true,
            operation: operation.as_str().into(),
            handlers: DefaultHandlers::default(),
            states,
        }
    }

    /// Return the MQTT message to register support for the operation described by this workflow
    pub fn capability_message(
        &self,
        schema: &MqttSchema,
        target: &EntityTopicId,
    ) -> Option<MqttMessage> {
        match self.operation {
            // Custom operations (and restart) have a generic empty capability message
            OperationType::Custom(_) | OperationType::Restart | OperationType::DeviceProfile => {
                let meta_topic = schema.capability_topic_for(target, self.operation.clone());
                let payload = "{}".to_string();
                Some(
                    MqttMessage::new(&meta_topic, payload)
                        .with_retain()
                        .with_qos(QoS::AtLeastOnce),
                )
            }
            // Builtin operations dynamically publish their capability message,
            // notably to include a list of supported types.
            _ => None,
        }
    }

    /// Return the action to be performed on a given state
    pub fn get_action(
        &self,
        command_state: &GenericCommandState,
    ) -> Result<OperationAction, WorkflowExecutionError> {
        self.states
            .get(&command_state.status)
            .ok_or_else(|| WorkflowExecutionError::UnknownStep {
                operation: (&self.operation).into(),
                step: command_state.status.clone(),
            })
            .map(|action| action.inject_state(command_state))
    }
}

impl OperationAction {
    pub fn inject_state(&self, state: &GenericCommandState) -> Self {
        match self {
            OperationAction::Script(script, handlers) => OperationAction::Script(
                Self::inject_values_into_script(state, script),
                handlers.clone(),
            ),
            OperationAction::BgScript(script, handlers) => OperationAction::BgScript(
                Self::inject_values_into_script(state, script),
                handlers.clone(),
            ),
            OperationAction::Operation(operation_expr, optional_script, input, handlers) => {
                let operation = state.inject_values_into_template(operation_expr);
                let optional_script = optional_script
                    .as_ref()
                    .map(|script| Self::inject_values_into_script(state, script));
                OperationAction::Operation(
                    operation,
                    optional_script,
                    input.clone(),
                    handlers.clone(),
                )
            }
            _ => self.clone(),
        }
    }

    fn inject_values_into_script(state: &GenericCommandState, script: &ShellScript) -> ShellScript {
        ShellScript {
            command: state.inject_values_into_template(&script.command),
            args: state.inject_values_into_parameters(&script.args),
        }
    }

    pub fn process_iterate(
        state: GenericCommandState,
        json_path: &str,
        handlers: IterateHandlers,
    ) -> Result<GenericCommandState, IterationError> {
        // Extract the array
        let Some(target) = state.extract_value(json_path) else {
            return Err(IterationError::InvalidTarget(json_path.to_string()));
        };

        let Some(items) = target.as_array() else {
            return Err(IterationError::TargetNotArray(json_path.to_string()));
        };

        if items.is_empty() {
            info!("Nothing to iterate as operations array is empty");
            return Ok(state.update(handlers.on_success));
        }

        // Check for the presence of the next_operation key
        let next_item = if let Some(next_item) = state.payload.get("@next") {
            let mut next_item = next_item.clone();
            let index = next_item.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

            // Validate the index
            if index >= items.len() {
                return Err(IterationError::IndexOutOfBounds(index));
            }

            let mut next_index = index + 1;
            loop {
                if next_index >= items.len() {
                    info!("Iteration finished");
                    return Ok(state.update(handlers.on_success));
                }

                let next = &items[next_index];
                let skipped = next.get("@skip").and_then(|v| v.as_bool()).unwrap_or(false);
                if skipped {
                    next_index += 1;
                    continue;
                } else {
                    break;
                }
            }

            next_item["index"] = json!(next_index);
            next_item["item"] = items[next_index].clone();

            next_item.clone()
        } else {
            // If next_operation does not exist, create it with index 0
            json!({
                "index": 0,
                "item": items[0].clone()
            })
        };

        let next_operation_json = json!({
            "@next": next_item
        });
        let new_state = state.update_with_json(next_operation_json);

        let new_state = new_state.update(handlers.on_next);

        Ok(new_state)
    }

    /// Rewrite a command state before pushing it to a builtin operation actor
    ///
    /// Return the command state unchanged if there is no appropriate substitute.
    pub fn adapt_builtin_request(&self, command_state: GenericCommandState) -> GenericCommandState {
        match self {
            OperationAction::BuiltInOperation(_, _) => {
                command_state.update(GenericStateUpdate::scheduled())
            }
            _ => command_state,
        }
    }

    /// Rewrite the command state returned by a builtin operation actor
    ///
    /// Depending the operation is executing, successful or failed,
    /// set the new state using the user provided handlers
    ///
    /// Return the command state unchanged if there is no appropriate handlers.
    pub fn adapt_builtin_response(
        &self,
        command_state: GenericCommandState,
    ) -> GenericCommandState {
        match self {
            OperationAction::BuiltIn(exec_handlers, _)
            | OperationAction::BuiltInOperation(_, exec_handlers)
                if command_state.is_executing() =>
            {
                command_state.update(exec_handlers.on_exec.clone())
            }
            OperationAction::BuiltIn(_, await_handlers)
            | OperationAction::AwaitOperationCompletion(await_handlers, _)
                if command_state.is_successful() =>
            {
                command_state.update(await_handlers.on_success.clone())
            }
            OperationAction::BuiltIn(_, await_handlers)
            | OperationAction::AwaitOperationCompletion(await_handlers, _)
                if command_state.is_failed() =>
            {
                let mut on_error = await_handlers.on_error.clone();
                if on_error.reason.is_none() {
                    if let Some(builtin_reason) = command_state.failure_reason() {
                        on_error.reason = Some(builtin_reason.to_string());
                    }
                }
                command_state.update(on_error)
            }
            _ => command_state,
        }
    }
}

#[derive(thiserror::Error, Debug, Eq, PartialEq)]
pub enum IterationError {
    #[error("No object found at {0}")]
    InvalidTarget(String),

    #[error("Object found at {0} is not an array")]
    TargetNotArray(String),

    #[error("Index: {0} is out of bounds")]
    IndexOutOfBounds(usize),
}

#[cfg(test)]
mod tests {
    use super::GenericCommandState;
    use super::GenericStateUpdate;
    use super::IterateHandlers;
    use super::IterationError;
    use super::OperationAction;
    use assert_json_diff::assert_json_eq;
    use assert_json_diff::assert_json_include;
    use assert_matches::assert_matches;
    use serde_json::json;

    #[test]
    fn test_iterate_first_iteration() {
        let handlers = IterateHandlers::new(
            "apply_operation".into(),
            GenericStateUpdate::successful(),
            GenericStateUpdate::failed("bad input".to_string()),
        );

        let state = GenericCommandState::new(
            "test/topic".try_into().unwrap(),
            "next_operation".to_string(),
            json!({
                "status": "next_operation",
                "operations": [
                    {
                        "operation": "software_update",
                        "payload": {
                            "key": "value"
                        }
                    }
                ]
            }),
        );

        let new_state =
            OperationAction::process_iterate(state, ".payload.operations", handlers).unwrap();

        assert_eq!(new_state.status, "apply_operation");
        assert_json_eq!(
            new_state.payload,
            json!({
                "status": "apply_operation",
                "operations": [
                    {
                        "operation": "software_update",
                        "payload": {
                            "key": "value"
                        }
                    }
                ],
                "@next": {
                    "index": 0,
                    "item": {
                        "operation": "software_update",
                        "payload": {
                            "key": "value"
                        }
                    }
                }
            })
        );
    }

    #[test]
    fn test_iterate_intermediate_iteration() {
        let handlers = IterateHandlers::new(
            "apply_operation".into(),
            GenericStateUpdate::successful(),
            GenericStateUpdate::failed("bad input".to_string()),
        );

        let state = GenericCommandState::new(
            "test/topic".try_into().unwrap(),
            "next_operation".to_string(),
            json!({
                "status": "next_operation",
                "operations": [
                    {
                        "operation": "firmware_update",
                        "payload": {
                            "firmware_key": "firmware_value"
                        }
                    },
                    {
                        "operation": "software_update",
                        "payload": {
                            "software_key": "software_value"
                        }
                    },
                    {
                        "operation": "config_update",
                        "payload": {
                            "config_key": "config_value"
                        }
                    }
                ],
                "@next": {
                    "index": 1,
                    "item": {
                        "operation": "software_update",
                        "payload": {
                            "software_key": "software_value"
                        }
                    }
                }
            }),
        );
        let new_state =
            OperationAction::process_iterate(state, ".payload.operations", handlers).unwrap();

        assert_eq!(new_state.status, "apply_operation");
        assert_json_eq!(
            new_state.payload,
            json!({
                "status": "apply_operation",
                "operations": [
                    {
                        "operation": "firmware_update",
                        "payload": {
                            "firmware_key": "firmware_value"
                        }
                    },
                    {
                        "operation": "software_update",
                        "payload": {
                            "software_key": "software_value"
                        }
                    },
                    {
                        "operation": "config_update",
                        "payload": {
                            "config_key": "config_value"
                        }
                    }
                ],
                "@next": {
                    "index": 2,
                    "item": {
                        "operation": "config_update",
                        "payload": {
                            "config_key": "config_value"
                        }
                    }
                }
            })
        );
    }

    #[test]
    fn test_iterate_final_iteration() {
        let handlers = IterateHandlers::new(
            "apply_operation".into(),
            GenericStateUpdate::successful(),
            GenericStateUpdate::failed("bad input".to_string()),
        );

        let state = GenericCommandState::new(
            "test/topic".try_into().unwrap(),
            "next_operation".to_string(),
            json!({
                "status": "next_operation",
                "operations": [
                    {
                        "operation": "firmware_update",
                        "payload": {
                            "firmware_key": "firmware_value"
                        }
                    },
                    {
                        "operation": "software_update",
                        "payload": {
                            "software_key": "software_value"
                        }
                    },
                    {
                        "operation": "config_update",
                        "payload": {
                            "config_key": "config_value"
                        }
                    }
                ],
                "@next": {
                    "index": 2,
                    "item": {
                        "operation": "config_update",
                        "payload": {
                            "config_key": "config_value"
                        }
                    }
                }
            }),
        );

        let new_state =
            OperationAction::process_iterate(state, ".payload.operations", handlers).unwrap();

        let expected_payload = json!({
            "status": "successful",
            "operations": [
                {
                    "operation": "firmware_update",
                    "payload": {
                        "firmware_key": "firmware_value"
                    }
                },
                {
                    "operation": "software_update",
                    "payload": {
                        "software_key": "software_value"
                    }
                },
                {
                    "operation": "config_update",
                    "payload": {
                        "config_key": "config_value"
                    }
                }
            ],
            "@next": {
                "index": 2,
                "item": {
                    "operation": "config_update",
                    "payload": {
                        "config_key": "config_value"
                    }
                }
            }
        });

        assert_eq!(new_state.status, "successful");
        assert_json_eq!(new_state.payload, expected_payload);
    }

    #[test]
    fn test_iterate_failed_iteration() {
        let handlers = IterateHandlers::new(
            "apply_operation".into(),
            GenericStateUpdate::successful(),
            GenericStateUpdate::failed("bad input".to_string()),
        );

        let state = GenericCommandState::new(
            "test/topic".try_into().unwrap(),
            "next_operation".to_string(),
            json!({
                "status": "next_operation",
                "operations": [
                    {
                        "operation": "config_update",
                        "payload": {}
                    }
                ],
                "@next": {
                    "index": 1
                }
            }),
        );

        let res = OperationAction::process_iterate(state, ".payload.operations", handlers);
        assert_matches!(res, Err(IterationError::IndexOutOfBounds(1)))
    }

    #[test]
    fn test_iterate_empty_array() {
        let handlers = IterateHandlers::new(
            "apply_operation".into(),
            GenericStateUpdate::successful(),
            GenericStateUpdate::failed("bad input".to_string()),
        );

        let state = GenericCommandState::new(
            "test/topic".try_into().unwrap(),
            "next_operation".to_string(),
            json!({
                "status": "next_operation",
                "operations": []
            }),
        );

        let new_state =
            OperationAction::process_iterate(state, ".payload.operations", handlers).unwrap();

        assert_eq!(new_state.status, "successful");
        assert_json_eq!(
            new_state.payload,
            json!({
                "status": "successful",
                "operations": []
            })
        );
    }

    #[test]
    fn test_iterate_target_not_array() {
        let handlers = IterateHandlers::new(
            "apply_operation".into(),
            GenericStateUpdate::successful(),
            GenericStateUpdate::failed("bad input".to_string()),
        );

        let state = GenericCommandState::new(
            "test/topic".try_into().unwrap(),
            "next_operation".to_string(),
            json!({
                "status": "next_operation",
                "operations": {}
            }),
        );

        let res = OperationAction::process_iterate(state, ".payload.operations", handlers);
        assert_matches!(res, Err(IterationError::TargetNotArray(_)))
    }

    #[test]
    fn test_iterate_invalid_target() {
        let handlers = IterateHandlers::new(
            "apply_operation".into(),
            GenericStateUpdate::successful(),
            GenericStateUpdate::failed("bad input".to_string()),
        );

        let state = GenericCommandState::new(
            "test/topic".try_into().unwrap(),
            "next_operation".to_string(),
            json!({
                "status": "next_operation",
                "operations": []
            }),
        );

        let res = OperationAction::process_iterate(state, ".bad.json.path", handlers);
        assert_matches!(res, Err(IterationError::InvalidTarget(_)))
    }

    #[test]
    fn test_skipping_entries_during_iteration() {
        let handlers = IterateHandlers::new(
            "apply_operation".into(),
            GenericStateUpdate::successful(),
            GenericStateUpdate::failed("bad input".to_string()),
        );

        let state = GenericCommandState::new(
            "test/topic".try_into().unwrap(),
            "next_operation".to_string(),
            json!({
                "status": "next_operation",
                "operations": [
                    {
                        "operation": "firmware_update",
                        "payload": {
                            "firmware_key": "firmware_value"
                        }
                    },
                    {
                        "@skip": false, // Explicitly not skip
                        "operation": "software_update",
                        "payload": {
                            "software_key": "software_value"
                        }
                    },
                    {
                        "operation": "software_update",
                        "@skip": true,  // Skip this entry
                        "payload": {
                            "skipped_software_key": "skipped_software_value"
                        }
                    },
                    {
                        "@skip": "bad_skip_value_type", // Interpreted as not `true`: do not skip
                        "operation": "config_update",
                        "payload": {
                            "config_key": "config_value"
                        }
                    }
                ]
            }),
        );

        // Iterate to the first operation
        let next_state =
            OperationAction::process_iterate(state, ".payload.operations", handlers.clone())
                .unwrap();

        // Iterate to the second operation that is explicitly not skipped
        let next_state =
            OperationAction::process_iterate(next_state, ".payload.operations", handlers.clone())
                .unwrap();

        assert_eq!(next_state.status, "apply_operation");
        assert_json_include!(
            actual: next_state.payload,
            expected: json!({
                "@next": {
                    "index": 1,
                    "item": {
                        "operation": "software_update",
                        "@skip": false,
                        "payload": {
                            "software_key": "software_value"
                        }
                    }
                }
            })
        );

        // Iterate to the next operation that has non-boolean `skip` value, which is not skipped
        let next_state =
            OperationAction::process_iterate(next_state, ".payload.operations", handlers).unwrap();

        assert_eq!(next_state.status, "apply_operation");
        assert_json_include!(
            actual: next_state.payload,
            expected: json!({
                "@next": {
                    "index": 3,
                    "item": {
                        "operation": "config_update",
                        "@skip": "bad_skip_value_type",
                        "payload": {
                            "config_key": "config_value"
                        }
                    }
                }
            })
        );
    }
}
