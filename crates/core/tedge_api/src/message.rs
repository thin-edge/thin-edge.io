use std::fmt::Debug;

/// A message exchanged between two plugins
pub trait Message: Debug + Clone + Eq + PartialEq {}

/// A message with an id
pub struct Envelop<M: Message> {
    id: String,
    message: M,
}
