use crate::filters::*;
use std::marker::PhantomData;

/// A filter that lets every message pass the filter.
pub struct PassthroughMessageFilter<T: Message> {
    _phantom: PhantomData<T>,
}

impl<T: Message> PassthroughMessageFilter<T> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<T: Message> MessageFilter for PassthroughMessageFilter<T> {
    type Message = T;

    fn filter(&mut self, _message: &Envelope<T>) -> FilterDecision {
        FilterDecision::Accept
    }
}
