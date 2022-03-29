use std::fmt::Debug;

/// A message exchanged between two plugins
pub trait Message: 'static + Clone + Debug + Send {}
