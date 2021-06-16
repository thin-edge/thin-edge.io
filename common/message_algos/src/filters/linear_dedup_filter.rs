//! Message deduplication

use crate::{filters::*, *};
use std::collections::hash_map::DefaultHasher;
use std::collections::VecDeque;
use std::hash::Hasher;

/// A linear message deduper.
///
/// Complexity: O(n)
pub struct LinearDedupFilter<T: Message> {
    max_history_capacity: usize,
    history: VecDeque<Entry<T>>,
}

struct Entry<T: Message> {
    hash: u64,
    envelope: Envelope<T>,
}

impl<T: Message> LinearDedupFilter<T> {
    pub fn new(max_history_capacity: usize) -> Self {
        assert!(max_history_capacity > 0);
        Self {
            max_history_capacity,
            history: VecDeque::with_capacity(max_history_capacity),
        }
    }
}

impl<T: Message> MessageFilter for LinearDedupFilter<T> {
    type Message = T;

    fn filter(&mut self, envelope: &Envelope<Self::Message>) -> FilterDecision {
        let message = &envelope.message;
        let hash = {
            let mut hasher = DefaultHasher::new();
            message.hash(&mut hasher);
            hasher.finish()
        };

        match self
            .history
            .iter()
            .find(|entry| entry.hash.eq(&hash) && entry.envelope.message.eq(&message))
        {
            Some(_) => FilterDecision::Reject,
            None => {
                if self.history.len() >= self.max_history_capacity {
                    // Make room. Drop oldest entry
                    let _ = self.history.pop_front();
                }
                self.history.push_back(Entry {
                    hash,
                    envelope: envelope.clone(),
                });
                FilterDecision::Accept
            }
        }
    }
}
