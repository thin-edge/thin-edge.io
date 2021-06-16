/// Tag type for messages we are dealing with.
///
/// Simplify trait bounds.
pub trait Message: Send + Clone + PartialEq + std::hash::Hash {}
