use std::fmt::Debug;

/// A message exchanged between two plugins
pub trait Message: Debug + Clone + Eq + PartialEq {}
