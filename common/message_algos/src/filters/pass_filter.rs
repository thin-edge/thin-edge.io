use crate::filters::*;
use std::marker::PhantomData;

/// A filter that lets every message pass the filter.
pub struct PassMessageFilter<T: Send> {
    _phantom: PhantomData<T>,
}

impl<T: Send> PassMessageFilter<T> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<T: Send> MessageFilter<T> for PassMessageFilter<T> {
    fn filter(&mut self, _message: &T) -> FilterDecision {
        FilterDecision::Accept
    }
}
