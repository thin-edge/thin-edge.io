/// Error preventing a workflow to be registered
#[derive(thiserror::Error, Debug, Eq, PartialEq)]
pub enum WorkflowDefinitionError {
    #[error("Missing mandatory state: {state}")]
    MissingState { state: String },

    #[error("Missing transition for state: {state}")]
    MissingTransition { state: String },

    #[error(transparent)]
    ScriptDefinitionError(#[from] ScriptDefinitionError),

    #[error("Unknown action: {action}")]
    UnknownAction { action: String },
}

/// Error related to a script definition
#[derive(thiserror::Error, Debug, Eq, PartialEq)]
pub enum ScriptDefinitionError {
    #[error("Error handler provided for 'on_error' and 'on_exit._'")]
    DuplicatedOnErrorHandler,

    #[error("Successful handler provided for 'on_success' and 'on_exit.0'")]
    DuplicatedOnSuccessHandler,

    #[error("Successful handler provided for 'on_success' and 'on_stdout'")]
    DuplicatedOnStdoutHandler,

    #[error("Overlapping handlers provided for '{first}' and 'second' exit code ranges")]
    OverlappingHandler { first: String, second: String },

    #[error("Invalid exit code range '{from}-{to}' as {from}>{to}")]
    IncorrectRange { from: u8, to: u8 },
}

/// Error preventing a workflow to be registered
#[derive(thiserror::Error, Debug, Eq, PartialEq)]
pub enum WorkflowRegistrationError {
    #[error("A workflow for this operation is already registered: {operation}")]
    DuplicatedWorkflow { operation: String },
}

/// Error preventing to infer the current action for an operation instance
#[derive(thiserror::Error, Debug)]
pub enum WorkflowExecutionError {
    #[error("No a command topic: {topic}")]
    InvalidCmdTopic { topic: String },

    #[error("The command payload is not a JSON object")]
    InvalidPayload(#[from] serde_json::Error),

    #[error("Missing status in the command payload")]
    MissingStatus,

    #[error("No workflow is defined for the operation: {operation}")]
    UnknownOperation { operation: String },

    #[error("No command has been initiated on the command topic: {topic}")]
    UnknownRequest { topic: String },

    #[error("Two concurrent requests are under execution on the same topic: {topic}")]
    DuplicatedRequest { topic: String },

    #[error("No such step is defined for {operation}: {step}")]
    UnknownStep { operation: String, step: String },
}
