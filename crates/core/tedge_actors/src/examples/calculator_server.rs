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
#[cfg(feature = "test-helpers")]
mod tests {
    use crate::examples::calculator_server::*;
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
            ServerActor::new(Calculator::default(), service_box_builder.build())
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
