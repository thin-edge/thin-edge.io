use std::fmt::Debug;

/// A message exchanged between two actors
pub trait Message: Debug + Send + 'static {}

/// There is no need to tag messages as such
impl<T: Debug + Send + 'static> Message for T {}

/// A type to tell no message is received or sent
#[derive(Debug)]
pub enum NoMessage {}
