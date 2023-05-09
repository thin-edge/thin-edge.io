//! Let's implement a client actor for the calculator server actor.
//!
//! ```
//! # use async_trait::async_trait;
//! # use tedge_actors::{Actor, ClientMessageBox, ServerActor, SimpleMessageBox, MessageReceiver, Sender, RuntimeError};
//! # use crate::tedge_actors::examples::calculator::*;
//!
//! /// An actor that send operations to a calculator server actor to reach a given target.
//! struct Player {
//!     name: String,
//!     target: i64,
//!     /// This actor use a simple message box
//!     /// to send `Operation` messages and to receive `Update` messages.
//!     ///
//!     /// Presumably this actor interacts with a `Calculator`
//!     /// and will have to send an `Operation` before receiving in return an `Update`
//!     /// But nothing enforces that. The message box only tell what is sent and received.
//!     messages: SimpleMessageBox<Update,Operation>,
//! }
//!
//! #[async_trait]
//! impl Actor for Player {
//!     fn name(&self) -> &str {
//!         &self.name
//!     }
//!
//!     async fn run(&mut self) -> Result<(), RuntimeError> {
//!         // Send a first identity `Operation` to see where we are.
//!         self.messages.send(Operation::Add(0)).await?;
//!
//!         while let Some(status) = self.messages.recv().await {
//!             // Reduce by two the gap to the target
//!             let delta = self.target - status.to;
//!             self.messages.send(Operation::Add(delta / 2)).await?;
//!         }
//!
//!         Ok(())
//!     }
//! }
//! ```
//!
//! To connect such an actor to the calculator, one needs actor builders
//! to establish appropriate connections between the actor message boxes.
//!
//! ```
//! # use tedge_actors::{Actor, Builder, ChannelError, MessageReceiver, Sender, ServiceConsumer, NoConfig, ServerActor, ServerMessageBox, ServerMessageBoxBuilder, SimpleMessageBox, SimpleMessageBoxBuilder};
//! # use crate::tedge_actors::examples::calculator_server::*;
//! # #[tokio::main]
//! # async fn main_test() -> Result<(),ChannelError> {
//! #
//!
//! // Building a box to hold 16 pending requests for the calculator server actor
//! // Note that a server actor requires a specific type of message box.
//! let mut server_box_builder = ServerMessageBoxBuilder::new("Calculator", 16);
//!
//! // Building a box to hold one pending message for the player
//! // This actor never expects more than one message.
//! let mut player_1_box_builder = SimpleMessageBoxBuilder::new("Player 1", 1);
//!
//! // Connecting the two boxes, so the box built by the `player_box_builder`:
//! // - receives as input, the output messages sent from the server message box
//! // - sends output messages to the server message box as its input.
//! player_1_box_builder.set_connection(&mut server_box_builder);
//!
//! // It matters that the builder of the server box is a `ServerMessageBoxBuilder`:
//! // as this builder accepts multiple client actors to connect to the same server.
//! let mut player_2_box_builder = SimpleMessageBoxBuilder::new("Player 2", 1);
//! player_2_box_builder.set_connection(&mut server_box_builder);
//!
//! // One can then build the message boxes
//! let server_box: ServerMessageBox<Operation,Update> = server_box_builder.build();
//! let mut player_1_box = player_1_box_builder.build();
//! let mut player_2_box = player_2_box_builder.build();
//!
//! // Then spawn the server
//! let server = Calculator::default();
//! tokio::spawn(async move { ServerActor::new(server, server_box).run().await } );
//!
//! // And use the players' boxes to interact with the server actor.
//! // Note that, compared to the test above of the calculator server,
//! // - the players don't have to deal with client identifiers,
//! // - each player receives the responses to its requests,
//! // - the server processes the requests in the order they have been received,
//! // - the responses to a client are affected by the requests sent by the others.
//! player_1_box.send(Operation::Add(0)).await?;
//! player_2_box.send(Operation::Add(0)).await?;
//!
//! assert_eq!(player_1_box.recv().await, Some(Update{from:0,to:0}));
//! player_1_box.send(Operation::Add(10)).await?;
//!
//! assert_eq!(player_2_box.recv().await, Some(Update{from:0,to:0}));
//! player_2_box.send(Operation::Add(5)).await?;
//!
//! assert_eq!(player_1_box.recv().await, Some(Update{from:0,to:10}));
//! assert_eq!(player_2_box.recv().await, Some(Update{from:10,to:15}));
//! #
//! # Ok(())
//! # }
//! ```
pub mod calculator;
pub mod calculator_server;
