use crate::{grouping::*, Envelope, Timestamp};

/// Describes the behavior of a message grouper.
pub trait MessageGrouper {
    type Message: Send + Clone;

    /// Add a message into one of the groups, or create a new message group.
    fn add_message(&mut self, message: Envelope<Self::Message>);

    /// Retire groups. The decision is based on the current system time `now` and the retirement
    /// policy.
    fn retire_groups(&mut self, now: Timestamp) -> RetireGroups<Self::Message>;

    /// Flushes all groups.
    fn flush_groups(&mut self) -> Vec<MessageGroup<Self::Message>>;
}

/// Describes the action to retire groups and when to call `retire_groups` again.
pub struct RetireGroups<T: Send + Clone> {
    /// The groups to retire.
    pub retired_groups: Vec<MessageGroup<T>>,
    /// When to check next for retirement.
    pub next_check_at: Option<Timestamp>,
}
