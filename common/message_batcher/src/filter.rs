//! Message filtering

pub enum FilterDecision {
    Accept,
    Reject,
}

/// Stateful message filter that accepts or rejects messages based on some (hidden) criteria.
pub trait MessageFilter<T: Send>: Send {
    fn filter(&mut self, message: &T) -> FilterDecision;
}

pub struct NoMessageFilter<T: Send> {
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Send> NoMessageFilter<T> {
    pub fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T: Send> MessageFilter<T> for NoMessageFilter<T> {
    fn filter(&mut self, message: &T) -> FilterDecision {
        FilterDecision::Accept
    }
}
