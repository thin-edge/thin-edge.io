use crate::Sender;
// TODO: make examples respond to RuntimeRequests
use crate::message_boxes::MessageReceiver;
use crate::Actor;
use crate::RuntimeError;
use crate::Server;
use crate::SimpleMessageBox;
use async_trait::async_trait;

/// State of the calculator service
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

/// Implementation of the calculator behavior as an actor
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

/// Implementation of the calculator behavior as a service
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

    async fn run(&mut self) -> Result<(), RuntimeError> {
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

#[cfg(test)]
mod tests {
    use crate::examples::calculator::Calculator;
    use crate::examples::calculator::Operation;
    use crate::examples::calculator::Player;
    use crate::examples::calculator::Update;
    use crate::test_helpers::Probe;
    use crate::test_helpers::ProbeEvent::Recv;
    use crate::test_helpers::ProbeEvent::Send;
    use crate::test_helpers::ServiceConsumerExt;
    use crate::Actor;
    use crate::Builder;
    use crate::ChannelError;
    use crate::ServerActor;
    use crate::ServerMessageBoxBuilder;
    use crate::ServiceConsumer;
    use crate::SimpleMessageBoxBuilder;

    #[tokio::test]
    async fn observing_an_actor() -> Result<(), ChannelError> {
        // Build the actor message boxes
        let mut service_box_builder = ServerMessageBoxBuilder::new("Calculator", 16);
        let mut player_box_builder = SimpleMessageBoxBuilder::new("Player 1", 1);

        // Connect the two actor message boxes interposing a probe.
        let mut probe = Probe::new();
        player_box_builder
            .with_probe(&mut probe)
            .set_connection(&mut service_box_builder);

        // Spawn the actors
        tokio::spawn(async move {
            ServerActor::new(
                Calculator {
                    state: 0,
                    messages: SimpleMessageBoxBuilder::new("ServerActor", 1).build(),
                },
                service_box_builder.build(),
            )
            .run()
            .await
        });
        tokio::spawn(async move {
            Player {
                name: "Player".to_string(),
                target: 42,
                messages: player_box_builder.build(),
            }
            .run()
            .await
        });

        // Observe the messages sent and received by the player.
        assert_eq!(probe.observe().await, Send(Operation::Add(0)));
        assert_eq!(probe.observe().await, Recv(Update { from: 0, to: 0 }));
        assert_eq!(probe.observe().await, Send(Operation::Add(21)));
        assert_eq!(probe.observe().await, Recv(Update { from: 0, to: 21 }));
        assert_eq!(probe.observe().await, Send(Operation::Add(10)));
        assert_eq!(probe.observe().await, Recv(Update { from: 21, to: 31 }));
        assert_eq!(probe.observe().await, Send(Operation::Add(5)));
        assert_eq!(probe.observe().await, Recv(Update { from: 31, to: 36 }));
        assert_eq!(probe.observe().await, Send(Operation::Add(3)));
        assert_eq!(probe.observe().await, Recv(Update { from: 36, to: 39 }));
        assert_eq!(probe.observe().await, Send(Operation::Add(1)));
        assert_eq!(probe.observe().await, Recv(Update { from: 39, to: 40 }));
        assert_eq!(probe.observe().await, Send(Operation::Add(1)));
        assert_eq!(probe.observe().await, Recv(Update { from: 40, to: 41 }));
        assert_eq!(probe.observe().await, Send(Operation::Add(0)));
        assert_eq!(probe.observe().await, Recv(Update { from: 41, to: 41 }));

        Ok(())
    }
}
