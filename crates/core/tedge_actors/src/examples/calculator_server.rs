//! Example: implementing a Calculator as an [Server]
pub use crate::examples::calculator::Operation;
pub use crate::examples::calculator::Update;
use crate::Actor;
use crate::MessageReceiver;
use crate::RuntimeError;
use crate::Sender;
use crate::Server;
use crate::SimpleMessageBox;
use async_trait::async_trait;

/// State of the calculator server
#[derive(Default)]
pub struct Calculator {
    state: i64,
}

/// Implementation of the calculator behavior as a [Server]
#[async_trait]
impl Server for Calculator {
    type Request = Operation;
    type Response = Update;

    fn name(&self) -> &str {
        "Calculator"
    }

    async fn handle(&mut self, request: Self::Request) -> Self::Response {
        // Act accordingly to the request
        let from = self.state;
        let to = match request {
            Operation::Add(x) => from + x,
            Operation::Multiply(x) => from * x,
        };

        // Update the service state
        self.state = to;

        // Return the response
        Update { from, to }
    }
}

/// An actor that send operations to a calculator service to reach a given target.
pub struct Player {
    pub name: String,
    pub target: i64,
    pub messages: SimpleMessageBox<Update, Operation>,
}

#[async_trait]
impl Actor for Player {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        // Send a first identity `Operation` to see where we are.
        self.messages.send(Operation::Add(0)).await?;

        while let Some(status) = self.messages.recv().await {
            // Reduce by two the gap to the target
            let delta = self.target - status.to;
            self.messages.send(Operation::Add(delta / 2)).await?;
        }

        Ok(())
    }
}
