use std::fmt::Debug;

/// A message exchanged between two actors
pub trait Message: Debug + Send + Sync + 'static {}

/// There is no need to tag messages as such
impl<T: Debug + Send + Sync + 'static> Message for T {}

/// Message type used by actor with no inputs or no outputs
#[derive(Debug)]
pub enum NoMessage {}
