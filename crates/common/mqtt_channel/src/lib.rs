//! A library to connect the local MQTT bus, publish messages and subscribe topics.
//!
//! ```no_run
//! use mqtt_channel::{Config, Connection, Message, Topic, MqttError, StreamExt, SinkExt};
//! use std::convert::TryInto;
//!
//! #[tokio::main]
//! async fn main () -> Result<(), MqttError>{
//!     // A client subscribes to its topics on connect
//!     let input_topic = "test/input/topic".try_into()?;
//!     let config = Config::default().with_session_name("test_client").with_subscriptions(input_topic);
//!     let mut con = Connection::new(&config).await?;
//!
//!     // The connection is materialized by two channels
//!     let mut received_messages = con.received;
//!     let mut published_messages = con.published;
//!
//!     // Messages are published by sending them on the published channel
//!     let output_topic = "test/output/topic".try_into()?;
//!     published_messages.send(Message::new(&output_topic, "hello mqtt")).await?;
//!
//!     // Messages are received from the subscriptions on the received channel
//!     let message = received_messages.next().await.ok_or(MqttError::ReadOnClosedConnection)?;
//!     println!("{}", message.payload_str()?);
//!
//!     // The connection is closed on drop
//!     Ok(())
//! }
//! ```
#![forbid(unsafe_code)]

mod channel;
mod config;
mod connection;
mod errors;
mod messages;
mod session;
mod topics;

#[cfg(test)]
mod tests;

pub use channel::*;
pub use config::*;
pub use connection::*;
pub use errors::*;
pub use messages::*;
pub use session::*;
pub use topics::*;

pub use futures::channel::mpsc::UnboundedReceiver;
pub use futures::channel::mpsc::UnboundedSender;
pub use futures::Sink;
pub use futures::SinkExt;
pub use futures::Stream;
pub use futures::StreamExt;

pub use rumqttc::QoS;
