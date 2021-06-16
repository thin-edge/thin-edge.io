use crate::Timestamp;

/// A message envelope.
#[derive(Debug, Clone)]
pub struct Envelope<T: Send + Clone> {
    pub message: T,
    pub received_at: Timestamp,
}
