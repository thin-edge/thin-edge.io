//! Message filtering

pub mod pass_filter;

pub enum FilterDecision {
    Accept,
    Reject,
}

/// Stateful message filter that accepts or rejects messages based on some criteria.
pub trait MessageFilter<T: Send>: Send {
    fn filter(&mut self, _message: &T) -> FilterDecision;
}
