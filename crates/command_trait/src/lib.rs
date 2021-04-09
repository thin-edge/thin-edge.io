pub trait Command {
    type Error: std::error::Error;

    fn description(&self) -> String;

    fn execute(self, context: &ExecutionContext) -> Result<(), Self::Error>;
}

pub struct ExecutionContext {
    // Try to inject as much as possible as data into the command struct and not via the
    // ExecutionContext.
    // What belongs here are things like things related to logging or printing to `stdout`.
}
