//! Message filtering

pub use crate::Envelope;

pub enum FilterDecision {
    Accept,
    Reject,
}

/// Stateful message filter that accepts or rejects messages based on some criteria.
pub trait MessageFilter: Send {
    type Message: Send + Clone;

    fn filter(&mut self, _message: &Envelope<Self::Message>) -> FilterDecision;
}

pub mod passthrough;
