use std::fmt::Debug;

use crate::fan_in_message_type;
use crate::RuntimeRequest;

/// A message exchanged between two actors
pub trait Message: Debug + Send + Sync + 'static {}

/// There is no need to tag messages as such
impl<T: Debug + Send + Sync + 'static> Message for T {}

// TODO: maybe don't need this, if you don't any other message types just accept RuntimeRequests?
// FIXME: fix the rustdoc on macro
// Message type used by actor with no inputs or no outputs
fan_in_message_type!(NoMessage[RuntimeRequest]: Debug);
