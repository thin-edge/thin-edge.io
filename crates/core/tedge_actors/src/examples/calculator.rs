//! Example: implementing a Calculator as an [Actor]
use crate::message_boxes::MessageReceiver;
use crate::Actor;
use crate::RuntimeError;
use crate::Sender;
use crate::SimpleMessageBox;
use async_trait::async_trait;

/// State of the calculator actor
pub struct Calculator {
    state: i64,
    messages: SimpleMessageBox<Operation, Update>,
}

impl Calculator {
    pub fn new(messages: SimpleMessageBox<Operation, Update>) -> Self {
        Self { state: 0, messages }
    }
}

/// Input messages of the calculator service
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    Add(i64),
    Multiply(i64),
}

/// Output messages of the calculator service
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Update {
    pub from: i64,
    pub to: i64,
}

/// Implementation of the calculator behavior as an [Actor]
#[async_trait]
impl Actor for Calculator {
    fn name(&self) -> &str {
        "Calculator"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        while let Some(op) = self.messages.recv().await {
            // Process in turn each input message
            let from = self.state;
            let to = match op {
                Operation::Add(x) => from + x,
                Operation::Multiply(x) => from * x,
            };

            // Update the actor state
            self.state = to;

            // Send output messages
            self.messages.send(Update { from, to }).await?
        }
        Ok(())
    }
}
