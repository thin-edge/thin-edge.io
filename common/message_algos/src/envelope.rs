use crate::{Message, Timestamp};

/// A message envelope.
#[derive(Debug, Clone)]
pub struct Envelope<T: Message> {
    pub message: T,
    pub received_at: Timestamp,
}
