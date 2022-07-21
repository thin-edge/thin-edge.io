use std::fmt::Debug;

/// A message exchanged between two actors
pub trait Message: 'static + Debug + Send + Sync {}

/// Strings can be used as Message
impl Message for String {}

/// An actor can have no input or no output messages
#[derive(Clone, Debug)]
pub enum NoMessage {}
impl Message for NoMessage {}
