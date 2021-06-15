//! Message deduplication

use crate::filter::*;
use std::collections::VecDeque;

/// Test whether two messages are a duplicate.
pub trait IsDuplicate<T: Send>: Send {
    fn is_duplicate(&self, msg1: &T, msg2: &T) -> bool;
}

pub struct MessageDeduper<T> {
    dedup_cond: Box<dyn IsDuplicate<T>>,
    max_history_capacity: usize,
    history: VecDeque<T>,
}

impl<T: Send> MessageDeduper<T> {
    pub fn new(dedup_cond: Box<dyn IsDuplicate<T>>, max_history_capacity: usize) -> Self {
        assert!(max_history_capacity > 0);
        Self {
            dedup_cond,
            max_history_capacity,
            history: VecDeque::with_capacity(max_history_capacity),
        }
    }
}

impl<T: Clone + Send> MessageFilter<T> for MessageDeduper<T> {
    fn filter(&mut self, message: &T) -> FilterDecision {
        match self
            .history
            .iter()
            .find(|msg| self.dedup_cond.is_duplicate(msg, message))
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
