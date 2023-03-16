use std::fmt::Debug;
use std::hash::Hash;
use time::OffsetDateTime;

/// Implement this interface for the items that you want batched.
/// No items with the same key will go in the same batch.
/// The event_time of the item will determine how items are grouped,
/// dependent on how the batcher is configured.
pub trait Batchable: 'static + Debug + Send + Sync {
    type Key: Eq + Hash + Debug + Send + Sync;

    /// Define the uniqueness within a batch.
    fn key(&self) -> Self::Key;

    /// The time at which this item was created. This time is used to group items into a batch.
    fn event_time(&self) -> OffsetDateTime;
}
