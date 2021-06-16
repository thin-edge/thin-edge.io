//! Message deduplication

use crate::{filters::*, Envelope};
use std::collections::VecDeque;

/// Test whether two messages are a duplicates.
pub trait DedupPolicy<T: Send + Clone>: Send {
    fn is_duplicate(&self, msg1: &Envelope<T>, msg2: &Envelope<T>) -> bool;
}

/// A message deduper
pub struct MessageDeduper<T: Send + Clone> {
    dedup_policy: Box<dyn DedupPolicy<T>>,
    max_history_capacity: usize,
    history: VecDeque<Envelope<T>>,
}

impl<T: Send + Clone> MessageDeduper<T> {
    pub fn new(dedup_policy: Box<dyn DedupPolicy<T>>, max_history_capacity: usize) -> Self {
        assert!(max_history_capacity > 0);
        Self {
            dedup_policy,
            max_history_capacity,
            history: VecDeque::with_capacity(max_history_capacity),
        }
    }
}

impl<T: Clone + Send> MessageFilter for MessageDeduper<T> {
    type Message = T;

    fn filter(&mut self, message: &Envelope<Self::Message>) -> FilterDecision {
        match self
            .history
            .iter()
            .find(|msg| self.dedup_policy.is_duplicate(msg, message))
        {
            Some(_) => FilterDecision::Reject,
            None => {
                if self.history.len() >= self.max_history_capacity {
                    // Make room. Drop oldest entry
                    let _ = self.history.pop_front();
                }
                self.history.push_back(message.clone());
                FilterDecision::Accept
            }
        }
    }
}
