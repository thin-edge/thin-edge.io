use crate::Actor;
use crate::ChannelError;
use crate::MessageBox;
use crate::Service;
use crate::SimpleMessageBox;
use async_trait::async_trait;

/// State of the calculator service
#[derive(Default)]
pub struct Calculator {
    state: i64,
}

/// Input messages of the calculator service
#[derive(Debug)]
pub enum Operation {
    Add(i64),
    Multiply(i64),
}

/// Output messages of the calculator service
#[derive(Debug, Eq, PartialEq)]
pub struct Update {
    pub from: i64,
    pub to: i64,
}

/// Implementation of the calculator behavior as an actor
#[async_trait]
impl Actor for Calculator {
    type MessageBox = SimpleMessageBox<Operation, Update>;

    fn name(&self) -> &str {
        "Calculator"
    }

    async fn run(mut self, mut messages: Self::MessageBox) -> Result<(), ChannelError> {
        while let Some(op) = messages.recv().await {
            // Process in turn each input message
            let from = self.state;
            let to = match op {
                Operation::Add(x) => from + x,
                Operation::Multiply(x) => from * x,
            };

            // Update the actor state
            self.state = to;

            // Send output messages
            messages.send(Update { from, to }).await?
        }
        Ok(())
    }
}

/// Implementation of the calculator behavior as a service
#[async_trait]
impl Service for Calculator {
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
