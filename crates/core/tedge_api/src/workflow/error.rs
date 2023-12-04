/// Error preventing a workflow to be registered
#[derive(thiserror::Error, Debug)]
pub enum WorkflowDefinitionError {
    #[error("Missing mandatory state: {state}")]
    MissingState { state: String },

    #[error("Missing transition for state: {state}")]
    MissingTransition { state: String },
}

/// Error preventing a workflow to be registered
#[derive(thiserror::Error, Debug)]
pub enum WorkflowRegistrationError {
    #[error("A workflow for this operation is already registered: {operation}")]
    DuplicatedWorkflow { operation: String },
}

/// Error preventing to infer the current action for an operation instance
#[derive(thiserror::Error, Debug)]
pub enum WorkflowExecutionError {
    #[error("The command payload is not a JSON object")]
    InvalidPayload(#[from] serde_json::Error),

    #[error("Missing status in the command payload")]
    MissingStatus,

    #[error("No workflow is defined for the operation: {operation}")]
    UnknownOperation { operation: String },

    #[error("No such step is defined for {operation}: {step}")]
    UnknownStep { operation: String, step: String },
}
